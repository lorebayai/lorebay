use std::time::Duration;

use lorebay_base::run_job;
use tokio::sync::watch;

use lorebay_lib_core::{self as library, LibraryState};

pub(crate) async fn remove_abandoned_agent_files(
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
            tracing::debug!("abandoned file cleanup started");
            match remove_round(&state).await {
                Ok(removed) => tracing::info!(removed, "abandoned file cleanup finished"),
                Err(error) => tracing::error!(%error, "abandoned file cleanup failed"),
            }
        },
    )
    .await;
}

async fn remove_round(state: &LibraryState) -> Result<u64, library::AgentFileCleanupError> {
    let mut removed = 0;
    loop {
        let batch = state
            .run_write(async |write| library::remove_abandoned_agent_files(write).await)
            .await?;
        removed += batch;
        if batch < library::AGENT_FILE_CLEANUP_BATCH_SIZE as u64 {
            return Ok(removed);
        }
    }
}
