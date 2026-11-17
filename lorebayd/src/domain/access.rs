use sea_orm::{DatabaseConnection, DbErr, TransactionTrait};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::repository::account::AccountRepository;
use crate::repository::agent_authorization::AgentAuthorizationRepository;

use super::login_session::now_millis;
use super::token::{Claims, TokenKind};

#[derive(Debug, Error)]
pub(crate) enum AccessError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Unauthorized")]
    Unauthorized,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AgentCaller {
    pub account_id: String,
    pub authorization_id: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Caller {
    pub account_id: String,
    pub login_session_id: String,
}

pub(crate) async fn verify_agent(
    database: &DatabaseConnection,
    claims: Claims,
) -> Result<AgentCaller, AccessError> {
    let now = now_millis()?;
    if claims.kind != TokenKind::AgentAuthorization || claims.exp <= (now / 1000) as u64 {
        return Err(AccessError::Unauthorized);
    }
    let transaction = database.begin().await?;
    let authorization = AgentAuthorizationRepository::builder()
        .connection(&transaction)
        .build()
        .by_random_id(&claims.sid)
        .await?
        .filter(|authorization| authorization.refresh_expires_at > now)
        .ok_or(AccessError::Unauthorized)?;
    AccountRepository::builder()
        .connection(&transaction)
        .build()
        .by_id(&authorization.account_id)
        .await?
        .filter(|account| account.deleted_at.is_none() && account.username == claims.sub)
        .ok_or(AccessError::Unauthorized)?;
    transaction.commit().await?;
    Ok(AgentCaller::builder()
        .account_id(authorization.account_id)
        .authorization_id(authorization.id)
        .build())
}
