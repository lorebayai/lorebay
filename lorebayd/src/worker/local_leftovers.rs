use std::sync::Arc;
use std::time::Duration;

use lorebay_base::run_job;
use tokio::sync::watch;

use crate::storage::LocalDirectory;

pub(crate) async fn remove_local_leftovers(
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
            match storage.remove_local_leftovers(lifetime).await {
                Ok(removed) => tracing::info!(removed, "local file cleanup finished"),
                Err(error) => tracing::error!(%error, "local file cleanup failed"),
            }
        },
    )
    .await;
}
