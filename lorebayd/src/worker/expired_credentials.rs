use std::time::Duration;

use lorebay_base::run_job;
use sea_orm::DatabaseConnection;
use tokio::sync::watch;

use crate::domain;

pub(crate) async fn remove_expired_credentials(
    database: DatabaseConnection,
    interval: Duration,
    unused_client_lifetime: Duration,
    mut stopped: watch::Receiver<bool>,
) {
    run_job(
        interval,
        async move {
            let _ = stopped.changed().await;
        },
        || remove_round(&database, unused_client_lifetime),
    )
    .await;
}

async fn remove_round(database: &DatabaseConnection, unused_client_lifetime: Duration) {
    tracing::debug!("expired credential cleanup started");
    match domain::remove_expired_credentials(database, unused_client_lifetime).await {
        Ok(removed) => tracing::info!(
            authorization_codes = removed.authorization_codes,
            login_sessions = removed.login_sessions,
            agent_authorizations = removed.agent_authorizations,
            refresh_token_history = removed.refresh_token_history,
            unused_clients = removed.unused_clients,
            "expired credential cleanup finished"
        ),
        Err(error) => tracing::error!(%error, "expired credential cleanup failed"),
    }
}
