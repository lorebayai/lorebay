use std::collections::HashSet;
use std::time::SystemTime;

use futures_util::StreamExt;
use sea_orm::DbErr;
use thiserror::Error;

use crate::repository::AgentFileRepository;
use crate::{LibraryState, Write, WriteOperation};

use super::time::now_millis;

pub const AGENT_FILE_CLEANUP_BATCH_SIZE: usize = 100;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AgentFileCleanupError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AgentFileCleanupError {
    pub const fn code(&self) -> &'static str {
        "server.internal"
    }
}

pub async fn remove_abandoned_agent_files(
    write: &mut Write<'_>,
) -> Result<u64, AgentFileCleanupError> {
    write.begin_operation(None)?;
    let age = i64::try_from(
        write
            .state()
            .settings()
            .abandoned_agent_file_lifetime
            .as_millis(),
    )
    .map_err(anyhow::Error::from)?;
    let before = now_millis()?.saturating_sub(age);
    let locations = AgentFileRepository::builder()
        .connection(write.transaction())
        .build()
        .delete_abandoned(before, AGENT_FILE_CLEANUP_BATCH_SIZE as u64)
        .await?;
    let removed = locations.len() as u64;
    write.delete.extend(locations);
    write.record(WriteOperation::RemoveAbandonedAgentFiles, None);
    Ok(removed)
}

pub async fn remove_orphaned_files(state: &LibraryState) -> Result<u64, AgentFileCleanupError> {
    let before = SystemTime::now()
        .checked_sub(state.settings().orphaned_file_lifetime)
        .ok_or_else(|| anyhow::anyhow!("Orphan cutoff is out of range"))?;
    let mut batches = state.storage.list().chunks(AGENT_FILE_CLEANUP_BATCH_SIZE);
    let mut removed = 0;
    while let Some(batch) = batches.next().await {
        let locations: Vec<String> = batch
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .filter(|file| file.modified < before)
            .map(|file| file.location)
            .collect();
        if locations.is_empty() {
            continue;
        }
        let referenced = state
            .run_read(async |handle| {
                let found = AgentFileRepository::builder()
                    .connection(handle.transaction())
                    .build()
                    .referenced_locations(locations.clone())
                    .await?;
                Ok::<_, AgentFileCleanupError>(found.into_iter().collect::<HashSet<_>>())
            })
            .await?;
        for location in locations
            .into_iter()
            .filter(|location| !referenced.contains(location))
        {
            state.storage.delete(&location).await?;
            removed += 1;
        }
    }
    Ok(removed)
}
