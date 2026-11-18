use std::time::Duration;

use lorebay_base::run_job;
use tokio::sync::watch;

use lorebay_lib_core::{self as library, LibraryState};

pub(crate) async fn remove_orphaned_files(
    state: LibraryState,
    interval: Duration,
    mut stopped: watch::Receiver<bool>,
) {
    run_job(
        interval,
        async move {
            let _ = stopped.changed().await;
        },
        || async {
            tracing::debug!("orphaned file cleanup started");
            match library::remove_orphaned_files(&state).await {
                Ok(removed) => tracing::info!(removed, "orphaned file cleanup finished"),
                Err(error) => tracing::error!(%error, "orphaned file cleanup failed"),
            }
        },
    )
    .await;
}
