use sea_orm::{DatabaseConnection, DbErr, TransactionTrait};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::repository::account::AccountRepository;
use crate::repository::agent_sign_in::AgentSignInRepository;

use crate::time::now_millis;

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
    pub agent_sign_in_id: String,
    pub name: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct PersonCaller {
    pub account_id: String,
    pub person_sign_in_id: String,
}

pub(crate) async fn verify_agent(
    database: &DatabaseConnection,
    claims: Claims,
) -> Result<AgentCaller, AccessError> {
    let now = now_millis()?;
    if claims.kind != TokenKind::AgentSignIn || claims.exp <= (now / 1000) as u64 {
        return Err(AccessError::Unauthorized);
    }
    let transaction = database.begin().await?;
    let sign_in = AgentSignInRepository::builder()
        .connection(&transaction)
        .build()
        .by_random_id(&claims.sid)
        .await?
        .filter(|sign_in| sign_in.refresh_expires_at > now)
        .ok_or(AccessError::Unauthorized)?;
    AccountRepository::builder()
        .connection(&transaction)
        .build()
        .by_id(&sign_in.account_id)
        .await?
        .filter(|account| account.deleted_at.is_none() && account.username == claims.sub)
        .ok_or(AccessError::Unauthorized)?;
    transaction.commit().await?;
    Ok(AgentCaller::builder()
        .account_id(sign_in.account_id)
        .agent_sign_in_id(sign_in.id)
        .name(sign_in.name)
        .build())
}
