use std::collections::HashSet;
use std::time::SystemTime;

use futures_util::StreamExt;
use sea_orm::DbErr;
use thiserror::Error;

use crate::repository::ArtifactRepository;
use crate::{LibraryState, WriteOperation};

use super::time::now_millis;

pub const ARTIFACT_CLEANUP_BATCH_SIZE: usize = 100;

#[derive(Debug, Error)]
pub enum ArtifactCleanupError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl ArtifactCleanupError {
    pub const fn code(&self) -> &'static str {
        "server.internal"
    }
}

pub async fn remove_abandoned_artifacts(state: &LibraryState) -> Result<u64, ArtifactCleanupError> {
    let age = i64::try_from(state.settings().abandoned_artifact_lifetime.as_millis())
        .map_err(anyhow::Error::from)?;
    let before = now_millis()?.saturating_sub(age);
    state
        .run_write(None, async |write| {
            write.begin_operation()?;
            let locations = ArtifactRepository::builder()
                .connection(write.transaction())
                .build()
                .delete_abandoned(before, ARTIFACT_CLEANUP_BATCH_SIZE as u64)
                .await?;
            let removed = locations.len() as u64;
            write.delete.extend(locations);
            write.record(WriteOperation::RemoveAbandonedArtifacts, None);
            Ok(removed)
        })
        .await
}

pub async fn remove_orphaned_files(state: &LibraryState) -> Result<u64, ArtifactCleanupError> {
    let before = SystemTime::now()
        .checked_sub(state.settings().orphaned_file_lifetime)
        .ok_or_else(|| anyhow::anyhow!("Orphan cutoff is out of range"))?;
    let mut batches = state.storage.list().chunks(ARTIFACT_CLEANUP_BATCH_SIZE);
    let mut removed = 0;
    while let Some(batch) = batches.next().await {
        let files: Vec<_> = batch
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .filter(|file| file.modified < before)
            .collect();
        let locations: Vec<_> = files
            .iter()
            .filter_map(|file| file.location.to_str().map(str::to_owned))
            .collect();
        let referenced = if locations.is_empty() {
            HashSet::new()
        } else {
            state
                .run_read(async |handle| {
                    let found = ArtifactRepository::builder()
                        .connection(handle.transaction())
                        .build()
                        .referenced_locations(locations)
                        .await?;
                    Ok::<_, ArtifactCleanupError>(found.into_iter().collect())
                })
                .await?
        };
        for file in files {
            if !file
                .location
                .to_str()
                .is_some_and(|location| referenced.contains(location))
            {
                removed += u64::from(state.storage.delete_listed(&file).await?);
            }
        }
    }
    Ok(removed)
}
