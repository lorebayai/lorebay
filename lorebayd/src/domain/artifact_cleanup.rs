use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use futures_util::{StreamExt, pin_mut};
use sea_orm::{DatabaseConnection, DbErr};
use thiserror::Error;

use crate::repository::ArtifactRepository;
use crate::storage::{FileStorage, StoredFile, StoredFileKind};

use super::login_session::now_millis;
use super::write_transaction;

pub(crate) const ARTIFACT_CLEANUP_BATCH_SIZE: usize = 100;

#[derive(Debug, Error)]
pub(crate) enum ArtifactCleanupError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub(crate) async fn remove_abandoned_artifacts(
    database: &DatabaseConnection,
    storage: &impl FileStorage,
    lifetime: Duration,
) -> Result<u64, ArtifactCleanupError> {
    let before =
        now_millis()? - i64::try_from(lifetime.as_millis()).map_err(anyhow::Error::from)?;
    let transaction = write_transaction(database).await?;
    let repository = ArtifactRepository::builder()
        .connection(&transaction)
        .build();
    let rows = repository
        .abandoned(before, ARTIFACT_CLEANUP_BATCH_SIZE as u64)
        .await?;
    repository
        .delete_ids(rows.iter().map(|row| row.id.clone()).collect())
        .await?;
    transaction.commit().await?;
    for row in &rows {
        storage.delete(&row.storage_key).await?;
    }
    Ok(rows.len() as u64)
}

pub(crate) async fn remove_orphaned_files(
    database: &DatabaseConnection,
    storage: &impl FileStorage,
    lifetime: Duration,
) -> Result<u64, ArtifactCleanupError> {
    let before = SystemTime::now() - lifetime;
    let batches = storage.list().chunks(ARTIFACT_CLEANUP_BATCH_SIZE);
    pin_mut!(batches);
    let mut removed = 0;
    while let Some(batch) = batches.next().await {
        let files: Vec<StoredFile> = batch
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .filter(|file| file.modified < before)
            .collect();
        let locations = files
            .iter()
            .filter(|file| file.kind == StoredFileKind::Artifact)
            .filter_map(|file| file.name.to_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let referenced: HashSet<String> = if locations.is_empty() {
            HashSet::new()
        } else {
            ArtifactRepository::builder()
                .connection(database)
                .build()
                .referenced_locations(locations)
                .await?
                .into_iter()
                .collect()
        };
        for file in files {
            if file.kind == StoredFileKind::Temporary
                || file
                    .name
                    .to_str()
                    .is_none_or(|name| !referenced.contains(name))
            {
                removed += u64::from(storage.delete_listed(&file).await?);
            }
        }
    }
    Ok(removed)
}
