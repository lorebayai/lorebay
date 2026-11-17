use std::time::Duration;

use lorebay_base::run_job;
use sea_orm::DatabaseConnection;
use tokio::sync::watch;

use crate::domain;

pub(crate) async fn remove_expired_credentials(
    database: DatabaseConnection,
    interval: Duration,
    mut stopped: watch::Receiver<bool>,
) {
    run_job(
        interval,
        async move {
            let _ = stopped.changed().await;
        },
        || remove_round(&database),
    )
    .await;
}

async fn remove_round(database: &DatabaseConnection) {
    tracing::debug!("expired credential cleanup started");
    match domain::remove_expired_credentials(database).await {
        Ok(removed) => tracing::info!(
            authorization_codes = removed.authorization_codes,
            login_sessions = removed.login_sessions,
            agent_authorizations = removed.agent_authorizations,
            "expired credential cleanup finished"
        ),
        Err(error) => tracing::error!(%error, "expired credential cleanup failed"),
    }
}
