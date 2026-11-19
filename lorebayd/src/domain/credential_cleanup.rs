use std::time::Duration;

use sea_orm::{
    DatabaseConnection, DbErr, SqliteTransactionMode, TransactionOptions, TransactionTrait,
};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::time::now_millis;

use crate::repository::OAuthClientRepository;
use crate::repository::agent_authorization::AgentAuthorizationRepository;
use crate::repository::device_sign_in_request::DeviceSignInRequestRepository;
use crate::repository::login_session::LoginSessionRepository;
use crate::repository::oauth_code::OauthCodeRepository;
use crate::repository::refresh_token_history::RefreshTokenHistoryRepository;

#[derive(Debug, Error)]
pub(crate) enum CredentialCleanupError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct RemovedCredentials {
    pub authorization_codes: u64,
    pub login_sessions: u64,
    pub agent_authorizations: u64,
    pub refresh_token_history: u64,
    pub unused_clients: u64,
    pub device_sign_in_requests: u64,
}

pub(crate) async fn remove_expired_credentials(
    database: &DatabaseConnection,
    unused_client_lifetime: Duration,
) -> Result<RemovedCredentials, CredentialCleanupError> {
    let now = now_millis()?;
    let transaction = database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await?;
    let refresh_token_history = RefreshTokenHistoryRepository::builder()
        .connection(&transaction)
        .build()
        .delete_expired(now)
        .await?;
    let removed = RemovedCredentials::builder()
        .refresh_token_history(refresh_token_history)
        .device_sign_in_requests(
            DeviceSignInRequestRepository::builder()
                .connection(&transaction)
                .build()
                .delete_expired(now)
                .await?,
        )
        .authorization_codes(
            OauthCodeRepository::builder()
                .connection(&transaction)
                .build()
                .delete_expired(now)
                .await?,
        )
        .login_sessions(
            LoginSessionRepository::builder()
                .connection(&transaction)
                .build()
                .delete_expired(now)
                .await?,
        )
        .agent_authorizations(
            AgentAuthorizationRepository::builder()
                .connection(&transaction)
                .build()
                .delete_expired(now)
                .await?,
        )
        .unused_clients(
            OAuthClientRepository::new(&transaction)
                .delete_unused(
                    now - i64::try_from(unused_client_lifetime.as_millis())
                        .map_err(anyhow::Error::from)?,
                    now,
                )
                .await?,
        )
        .build();
    transaction.commit().await?;
    Ok(removed)
}
