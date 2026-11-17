use sea_orm::{
    DatabaseConnection, DbErr, SqliteTransactionMode, TransactionOptions, TransactionTrait,
};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::agent_authorization::Model;
use crate::repository::agent_authorization::AgentAuthorizationRepository;

use super::caller::{CallerError, VerifiedCaller};
use super::pagination::{Page, PageOptions, decode_cursor, encode_cursor};

#[derive(Debug, Error)]
pub(crate) enum AgentError {
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error("Not Found")]
    NotFound,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Agent {
    pub name: String,
    pub client_id: String,
    pub created_at: i64,
}

impl From<Model> for Agent {
    fn from(model: Model) -> Self {
        Self::builder()
            .name(model.name)
            .client_id(model.client_id)
            .created_at(model.created_at)
            .build()
    }
}

pub(crate) async fn list(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    options: PageOptions,
) -> Result<Page<Agent>, AgentError> {
    caller.person()?;
    let account_id = caller.account_id();
    let after = options
        .cursor
        .map(|cursor| {
            decode_cursor(CURSOR_VERSION, account_id, &cursor).ok_or(AgentError::InvalidCursor)
        })
        .transpose()?;
    let mut records = AgentAuthorizationRepository::builder()
        .connection(database)
        .build()
        .list(account_id, after.as_deref(), options.page_size + 1)
        .await?;
    let has_more = records.len() as u64 > options.page_size;
    records.truncate(options.page_size as usize);
    let next_cursor = records
        .last()
        .filter(|_| has_more)
        .map(|record| encode_cursor(CURSOR_VERSION, account_id, &record.name));
    Ok(Page::builder()
        .items(records.into_iter().map(Agent::from).collect::<Vec<_>>())
        .next_cursor(next_cursor)
        .build())
}

pub(crate) async fn revoke(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    name: &str,
) -> Result<(), AgentError> {
    caller.person()?;
    let transaction = database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await?;
    let deleted = AgentAuthorizationRepository::builder()
        .connection(&transaction)
        .build()
        .delete_by_name(caller.account_id(), name)
        .await?;
    if deleted == 0 {
        return Err(AgentError::NotFound);
    }
    transaction.commit().await?;
    Ok(())
}

const CURSOR_VERSION: &str = "agents-v1";
