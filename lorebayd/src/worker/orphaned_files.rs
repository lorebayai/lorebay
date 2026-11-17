use std::sync::Arc;
use std::time::Duration;

use lorebay_base::run_job;
use sea_orm::DatabaseConnection;
use tokio::sync::watch;

use crate::domain;
use crate::storage::LocalDirectory;

pub(crate) async fn remove_orphaned_files(
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
            tracing::debug!("orphaned file cleanup started");
            match domain::remove_orphaned_files(&database, storage.as_ref(), lifetime).await {
                Ok(removed) => tracing::info!(removed, "orphaned file cleanup finished"),
                Err(error) => tracing::error!(%error, "orphaned file cleanup failed"),
            }
        },
    )
    .await;
}
