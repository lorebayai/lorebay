use sea_orm::{
    DatabaseConnection, DbErr, SqliteTransactionMode, TransactionOptions, TransactionTrait,
};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::repository::agent_authorization::AgentAuthorizationRepository;
use crate::repository::login_session::LoginSessionRepository;
use crate::repository::oauth_code::OauthCodeRepository;
use crate::repository::refresh_token_history::RefreshTokenHistoryRepository;

use super::login_session::now_millis;

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
}

pub(crate) async fn remove_expired_credentials(
    database: &DatabaseConnection,
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
        .build();
    transaction.commit().await?;
    Ok(removed)
}
