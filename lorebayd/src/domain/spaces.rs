use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sea_orm::{
    DatabaseConnection, DatabaseTransaction, DbErr, IntoActiveModel, Set, SqlErr,
    SqliteTransactionMode, TransactionOptions, TransactionTrait,
};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::{participant, space};
use crate::repository::SpaceRepository;
use crate::repository::participant::ParticipantRepository;

use super::caller::{CallerError, VerifiedCaller};
use super::login_session::now_millis;
use super::pagination::{Page, PageOptions};
use super::token::token_hash;
use super::{AuthorizedSpace, SpaceAccessError, SpaceRole, check_space, primary_key};

#[derive(Debug, Error)]
pub(crate) enum SpaceError {
    #[error(transparent)]
    Access(#[from] SpaceAccessError),
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid cursor")]
    InvalidCursor,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Space {
    pub key: String,
    pub name: String,
    pub summary: String,
    pub role: SpaceRole,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct SpaceChanges {
    pub name: Option<String>,
    pub summary: Option<String>,
}

impl From<AuthorizedSpace> for Space {
    fn from(authorized: AuthorizedSpace) -> Self {
        Self::builder()
            .key(format!("SPACE-{}", authorized.space.number))
            .name(authorized.space.name)
            .summary(authorized.space.summary)
            .role(authorized.role)
            .build()
    }
}

pub(crate) async fn create_space(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    name: String,
    summary: String,
) -> Result<Space, SpaceError> {
    caller.person()?;
    let mut retries = NUMBER_CONFLICT_RETRIES;
    loop {
        let transaction = write_transaction(database).await?;
        match insert_space(&transaction, caller, &name, &summary).await {
            Ok(space) => {
                transaction.commit().await?;
                return Ok(space.into());
            }
            Err(SpaceError::Database(error))
                if retries > 0
                    && matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) =>
            {
                transaction.rollback().await?;
                retries -= 1;
            }
            Err(error) => return Err(error),
        }
    }
}

pub(crate) async fn list_spaces(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    options: PageOptions,
) -> Result<Page<Space>, SpaceError> {
    let after = options
        .cursor
        .map(|cursor| decode_cursor(&cursor, caller.account_id()))
        .transpose()?
        .unwrap_or(0);
    let mut records = SpaceRepository::builder()
        .connection(database)
        .build()
        .list_by_account(caller.account_id(), after, options.page_size + 1)
        .await?;
    let has_more = records.len() as u64 > options.page_size;
    records.truncate(options.page_size as usize);
    let next_cursor = records
        .last()
        .filter(|_| has_more)
        .map(|(space, _)| encode_cursor(space.number, caller.account_id()));
    let items = records
        .into_iter()
        .map(|(space, participant)| {
            let participant =
                participant.ok_or_else(|| DbErr::Type("Missing space participant".into()))?;
            let role = SpaceRole::for_caller(&participant.role, caller)?;
            Ok(AuthorizedSpace::builder()
                .space(space)
                .role(role)
                .build()
                .into())
        })
        .collect::<Result<Vec<_>, DbErr>>()?;
    Ok(Page::builder()
        .items(items)
        .next_cursor(next_cursor)
        .build())
}

pub(crate) async fn read_space(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    key: &str,
) -> Result<Space, SpaceError> {
    let transaction = database.begin().await?;
    let authorized = check_space(&transaction, caller, key, READ_ROLES).await?;
    transaction.commit().await?;
    Ok(authorized.into())
}

pub(crate) async fn change_space(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    key: &str,
    changes: SpaceChanges,
) -> Result<Space, SpaceError> {
    caller.person()?;
    let transaction = write_transaction(database).await?;
    let authorized = check_space(&transaction, caller, key, CHANGE_ROLES).await?;
    let mut model = authorized.space.into_active_model();
    if let Some(name) = changes.name {
        model.name = Set(name);
    }
    if let Some(summary) = changes.summary {
        model.summary = Set(summary);
    }
    model.updated_at = Set(now_millis()?);
    let space = SpaceRepository::builder()
        .connection(&transaction)
        .build()
        .update(model)
        .await?;
    transaction.commit().await?;
    Ok(AuthorizedSpace::builder()
        .space(space)
        .role(authorized.role)
        .build()
        .into())
}

const CURSOR_VERSION: &str = "spaces-v1";
const NUMBER_CONFLICT_RETRIES: u8 = 2;
const READ_ROLES: &[SpaceRole] = &[
    SpaceRole::Owner,
    SpaceRole::Admin,
    SpaceRole::Contributor,
    SpaceRole::Viewer,
];
const CHANGE_ROLES: &[SpaceRole] = &[SpaceRole::Owner, SpaceRole::Admin];

async fn insert_space(
    transaction: &DatabaseTransaction,
    caller: &VerifiedCaller,
    name: &str,
    summary: &str,
) -> Result<AuthorizedSpace, SpaceError> {
    let repository = SpaceRepository::builder().connection(transaction).build();
    let number = repository
        .highest_number()
        .await?
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| DbErr::Type("Space numbers exhausted".into()))?;
    let now = now_millis()?;
    let space = repository
        .insert(space::ActiveModel {
            id: Set(primary_key()),
            number: Set(number),
            name: Set(name.to_owned()),
            summary: Set(summary.to_owned()),
            next_task_number: Set(1),
            next_knowledge_number: Set(1),
            next_artifact_number: Set(1),
            next_proposal_number: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .await?;
    ParticipantRepository::builder()
        .connection(transaction)
        .build()
        .insert(participant::ActiveModel {
            id: Set(primary_key()),
            space_id: Set(space.id.clone()),
            account_id: Set(caller.account_id().to_owned()),
            role: Set(SpaceRole::Owner.as_str().to_owned()),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .await?;
    Ok(AuthorizedSpace::builder()
        .space(space)
        .role(SpaceRole::Owner)
        .build())
}

async fn write_transaction(database: &DatabaseConnection) -> Result<DatabaseTransaction, DbErr> {
    database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
}

fn encode_cursor(number: i64, account_id: &str) -> String {
    URL_SAFE_NO_PAD.encode(format!(
        "{CURSOR_VERSION}\n{}\n{number}",
        token_hash(account_id)
    ))
}

fn decode_cursor(cursor: &str, account_id: &str) -> Result<i64, SpaceError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| SpaceError::InvalidCursor)?;
    let decoded = String::from_utf8(bytes).map_err(|_| SpaceError::InvalidCursor)?;
    let mut parts = decoded.splitn(3, '\n');
    if parts.next() != Some(CURSOR_VERSION) || parts.next() != Some(token_hash(account_id).as_str())
    {
        return Err(SpaceError::InvalidCursor);
    }
    parts
        .next()
        .and_then(|number| number.parse().ok())
        .filter(|number| *number > 0)
        .ok_or(SpaceError::InvalidCursor)
}
