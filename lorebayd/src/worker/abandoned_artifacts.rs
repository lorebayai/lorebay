use std::sync::Arc;
use std::time::Duration;

use lorebay_base::run_job;
use sea_orm::DatabaseConnection;
use tokio::sync::watch;

use crate::domain;
use crate::storage::LocalDirectory;

pub(crate) async fn remove_abandoned_artifacts(
    database: DatabaseConnection,
    storage: Arc<LocalDirectory>,
    interval: Duration,
    lifetime: Duration,
    mut stopped: watch::Receiver<bool>,
) {
    run_job(
        interval,
        async move {
            let _ = stopped.changed().await;
        },
        || async {
            tracing::debug!("abandoned artifact cleanup started");
            match remove_round(&database, storage.as_ref(), lifetime).await {
                Ok(removed) => tracing::info!(removed, "abandoned artifact cleanup finished"),
                Err(error) => tracing::error!(%error, "abandoned artifact cleanup failed"),
            }
        },
    )
    .await;
}

async fn remove_round(
    database: &DatabaseConnection,
    storage: &LocalDirectory,
    lifetime: Duration,
) -> Result<u64, domain::ArtifactCleanupError> {
    let mut removed = 0;
    loop {
        let batch = domain::remove_abandoned_artifacts(database, storage, lifetime).await?;
        removed += batch;
        if batch < domain::ARTIFACT_CLEANUP_BATCH_SIZE as u64 {
            return Ok(removed);
        }
    }
}
