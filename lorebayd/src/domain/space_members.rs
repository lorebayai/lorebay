use lorebay_lib_core::{SpaceAccessError, SpaceReference, SpaceRole, resolve_space};
use sea_orm::{
    DatabaseConnection, DatabaseTransaction, DbErr, IntoActiveModel, Set, SqliteTransactionMode,
    TransactionOptions, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::member;
use crate::repository::account::AccountRepository;
use crate::repository::member::{MemberRecord, MemberRepository, RosterAfter};

use crate::time::now_millis;

use super::caller::{PersonSessionError, VerifiedCaller};
use super::pagination::{Page, PageOptions, decode_cursor, encode_cursor};
use super::primary_key;

#[derive(Debug, Error)]
pub(crate) enum SpaceMemberError {
    #[error(transparent)]
    Access(#[from] SpaceAccessError),
    #[error("The person is already in the space")]
    AlreadyExists,
    #[error(transparent)]
    Caller(#[from] PersonSessionError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error("Space member not found")]
    NotFound,
    #[error("User not found")]
    UserNotFound,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SpaceMemberKind {
    Agent,
    #[serde(rename = "human")]
    Person,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct SpaceMember {
    pub username: String,
    pub role: SpaceRole,
    pub agent_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct RosterPosition {
    pub username: String,
    pub kind: SpaceMemberKind,
    pub agent_name: String,
}

impl TryFrom<&str> for SpaceMemberKind {
    type Error = DbErr;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "agent" => Ok(Self::Agent),
            "human" => Ok(Self::Person),
            _ => Err(DbErr::Type("Invalid space member kind".into())),
        }
    }
}

impl SpaceMemberKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Person => "human",
        }
    }
}

impl From<&RosterPosition> for RosterAfter {
    fn from(position: &RosterPosition) -> Self {
        Self::builder()
            .username(&position.username)
            .kind(position.kind.as_str())
            .agent_name(&position.agent_name)
            .build()
    }
}

pub(crate) async fn read_role(
    transaction: &DatabaseTransaction,
    person_id: &str,
    space_id: &str,
) -> Result<Option<SpaceRole>, DbErr> {
    MemberRepository::builder()
        .connection(transaction)
        .build()
        .by_space_and_account(space_id, person_id)
        .await?
        .map(|row| {
            SpaceRole::try_from(row.role.as_str()).map_err(|error| DbErr::Type(error.to_string()))
        })
        .transpose()
}

pub(crate) async fn list_space_members(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    key: &str,
    options: PageOptions,
) -> Result<Page<SpaceMember>, SpaceMemberError> {
    let transaction = database.begin().await?;
    let authorized = check_space(&transaction, caller, key, READ_ROLES).await?;
    let version = format!("members-v2:{key}");
    let after = options
        .cursor
        .map(|cursor| {
            let value = decode_cursor(&version, caller.account_id(), &cursor)
                .ok_or(SpaceMemberError::InvalidCursor)?;
            let position: RosterPosition =
                serde_json::from_str(&value).map_err(|_| SpaceMemberError::InvalidCursor)?;
            if position.username.is_empty()
                || matches!(position.kind, SpaceMemberKind::Person)
                    && !position.agent_name.is_empty()
            {
                return Err(SpaceMemberError::InvalidCursor);
            }
            Ok(RosterAfter::from(&position))
        })
        .transpose()?;
    let mut records = MemberRepository::builder()
        .connection(&transaction)
        .build()
        .list(&authorized.space.id, after.as_ref(), options.page_size + 1)
        .await?;
    let has_more = records.len() as u64 > options.page_size;
    records.truncate(options.page_size as usize);
    let next_cursor = records
        .last()
        .filter(|_| has_more)
        .map(|record| roster_cursor(record, &version, caller.account_id()))
        .transpose()?;
    let items = records
        .into_iter()
        .map(space_member_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    transaction.commit().await?;
    Ok(Page::builder()
        .items(items)
        .next_cursor(next_cursor)
        .build())
}

pub(crate) async fn add_space_member(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    key: &str,
    username: &str,
    role: SpaceRole,
) -> Result<SpaceMember, SpaceMemberError> {
    caller.person()?;
    let transaction = write_transaction(database).await?;
    let authorized = check_space(&transaction, caller, key, MANAGE_ROLES).await?;
    if !matches!(role, SpaceRole::Contributor | SpaceRole::Viewer) {
        return Err(SpaceAccessError::PermissionRequired.into());
    }
    let account = AccountRepository::builder()
        .connection(&transaction)
        .build()
        .by_username(username)
        .await?
        .filter(|account| account.deleted_at.is_none())
        .ok_or(SpaceMemberError::UserNotFound)?;
    let repository = MemberRepository::builder().connection(&transaction).build();
    if repository
        .by_space_and_account(&authorized.space.id, &account.id)
        .await?
        .is_some()
    {
        return Err(SpaceMemberError::AlreadyExists);
    }
    let now = now_millis()?;
    repository
        .insert(member::ActiveModel {
            id: Set(primary_key()),
            space_id: Set(authorized.space.id),
            account_id: Set(account.id),
            space_number: Set(authorized.space.number),
            role: Set(role.as_str().to_owned()),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .await?;
    transaction.commit().await?;
    Ok(SpaceMember::builder()
        .username(username)
        .role(role)
        .agent_name(None)
        .build())
}

pub(crate) async fn change_space_member_role(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    key: &str,
    username: &str,
    role: SpaceRole,
) -> Result<SpaceMember, SpaceMemberError> {
    caller.person()?;
    let transaction = write_transaction(database).await?;
    let model = mutation_target(&transaction, caller, key, username, Some(role)).await?;
    let mut model = model.into_active_model();
    model.role = Set(role.as_str().to_owned());
    model.updated_at = Set(now_millis()?);
    MemberRepository::builder()
        .connection(&transaction)
        .build()
        .update(model)
        .await?;
    transaction.commit().await?;
    Ok(SpaceMember::builder()
        .username(username)
        .role(role)
        .agent_name(None)
        .build())
}

pub(crate) async fn remove_space_member(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    key: &str,
    username: &str,
) -> Result<(), SpaceMemberError> {
    caller.person()?;
    let transaction = write_transaction(database).await?;
    let model = mutation_target(&transaction, caller, key, username, None).await?;
    MemberRepository::builder()
        .connection(&transaction)
        .build()
        .delete(&model.id)
        .await?;
    transaction.commit().await?;
    Ok(())
}

const READ_ROLES: &[SpaceRole] = &[
    SpaceRole::Owner,
    SpaceRole::Admin,
    SpaceRole::Contributor,
    SpaceRole::Viewer,
];
const MANAGE_ROLES: &[SpaceRole] = &[SpaceRole::Owner, SpaceRole::Admin];

fn space_member_from_record(record: MemberRecord) -> Result<SpaceMember, DbErr> {
    let mut role = SpaceRole::try_from(record.role.as_str())
        .map_err(|error| DbErr::Type(error.to_string()))?;
    if matches!(
        SpaceMemberKind::try_from(record.kind.as_str())?,
        SpaceMemberKind::Agent
    ) {
        role = role.for_agent();
    }
    Ok(SpaceMember::builder()
        .username(record.username)
        .role(role)
        .agent_name(record.agent_name)
        .build())
}

fn roster_cursor(
    record: &MemberRecord,
    version: &str,
    account_id: &str,
) -> Result<String, SpaceMemberError> {
    let position = RosterPosition::builder()
        .username(&record.username)
        .kind(SpaceMemberKind::try_from(record.kind.as_str())?)
        .agent_name(record.agent_name.as_deref().unwrap_or_default())
        .build();
    let value = serde_json::to_string(&position).map_err(anyhow::Error::from)?;
    Ok(encode_cursor(version, account_id, &value))
}

async fn mutation_target(
    transaction: &DatabaseTransaction,
    caller: &VerifiedCaller,
    key: &str,
    username: &str,
    role: Option<SpaceRole>,
) -> Result<member::Model, SpaceMemberError> {
    let authorized = check_space(transaction, caller, key, MANAGE_ROLES).await?;
    let account = AccountRepository::builder()
        .connection(transaction)
        .build()
        .by_username(username)
        .await?
        .filter(|account| account.deleted_at.is_none())
        .ok_or(SpaceMemberError::NotFound)?;
    let target = MemberRepository::builder()
        .connection(transaction)
        .build()
        .by_space_and_account(&authorized.space.id, &account.id)
        .await?
        .ok_or(SpaceMemberError::NotFound)?;
    let target_role = SpaceRole::try_from(target.role.as_str())
        .map_err(|error| DbErr::Type(error.to_string()))?;
    if target_role == SpaceRole::Owner
        || role == Some(SpaceRole::Owner)
        || (authorized.role != SpaceRole::Owner
            && (target_role == SpaceRole::Admin || role == Some(SpaceRole::Admin)))
    {
        return Err(SpaceAccessError::PermissionRequired.into());
    }
    Ok(target)
}

async fn write_transaction(database: &DatabaseConnection) -> Result<DatabaseTransaction, DbErr> {
    database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
}

struct AuthorizedSpace {
    space: SpaceReference,
    role: SpaceRole,
}

async fn check_space(
    transaction: &DatabaseTransaction,
    caller: &VerifiedCaller,
    key: &str,
    allowed: &[SpaceRole],
) -> Result<AuthorizedSpace, SpaceAccessError> {
    let space = resolve_space(transaction, &caller.library_caller(), key).await?;
    let role = read_role(transaction, caller.account_id(), &space.id)
        .await?
        .ok_or(SpaceAccessError::NotFound)?;
    let role = if matches!(caller, VerifiedCaller::Agent(_)) {
        role.for_agent()
    } else {
        role
    };
    if !allowed.contains(&role) {
        return Err(SpaceAccessError::PermissionRequired);
    }
    Ok(AuthorizedSpace { space, role })
}
