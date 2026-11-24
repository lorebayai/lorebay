use std::slice::from_ref;

use sea_orm::{DatabaseTransaction, IntoActiveModel, Set, SqlErr};
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::entity::{space, space_knowledge as entry, space_knowledge_version as version};
use crate::repository::{KnowledgeRepository, KnowledgeVersionRepository, SpaceRepository};
use crate::{Caller, LibraryState, OperationHandle, Write, WriteOperation};

use super::id::primary_key;
use super::key::format_key;
use super::space_access::{READ_ROLES, SpaceRole, check_space};
use super::time::now_millis;
use super::{
    KnowledgeChanges, KnowledgeError, KnowledgeInputError, KnowledgeTarget, NewKnowledgeEntry,
    VersionedKnowledgeTarget,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KnowledgeVersionKind {
    Content,
    Deletion,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct KnowledgeEntry {
    pub key: String,
    pub name: String,
    pub version: i64,
    pub kind: KnowledgeVersionKind,
    pub summary: Option<String>,
    pub content: Option<String>,
    pub applied_by: String,
    pub applied_at: i64,
}

impl TryFrom<&str> for KnowledgeVersionKind {
    type Error = anyhow::Error;

    fn try_from(kind: &str) -> Result<Self, Self::Error> {
        match kind {
            "content" => Ok(Self::Content),
            "deletion" => Ok(Self::Deletion),
            _ => Err(anyhow::anyhow!("Invalid Knowledge version kind")),
        }
    }
}

pub async fn create_knowledge_entry(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    input: NewKnowledgeEntry,
) -> Result<KnowledgeEntry, KnowledgeError> {
    write.begin_operation(Some(caller))?;
    let space_id = direct_space(write, caller, space).await?;
    ensure_name_available(write.transaction(), &space_id, &input.name, None).await?;
    let entry = insert_entry(write.transaction(), &space_id, &input.name).await?;
    let version = append_version(
        write.transaction(),
        caller,
        &entry,
        Some((input.summary, input.content)),
        None,
    )
    .await?;
    let result = named_entry(write.state(), write.handle(), entry, version).await?;
    write.record(WriteOperation::CreateKnowledgeEntry, Some(space_id));
    Ok(result)
}

pub async fn change_knowledge_entry(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    target: VersionedKnowledgeTarget,
    changes: KnowledgeChanges,
) -> Result<KnowledgeEntry, KnowledgeError> {
    write.begin_operation(Some(caller))?;
    let space_id = direct_space(write, caller, space).await?;
    let entry = locked_entry(write.transaction(), &space_id, &target.target).await?;
    check_version(&entry, target.version)?;
    if let Some(name) = &changes.name {
        ensure_name_available(write.transaction(), &space_id, name, Some(&entry.id)).await?;
    }
    let entry = apply_changes(write.transaction(), caller, entry, changes, None).await?;
    let version = latest(write.transaction(), &entry).await?;
    let result = named_entry(write.state(), write.handle(), entry, version).await?;
    write.record(WriteOperation::ChangeKnowledgeEntry, Some(space_id));
    Ok(result)
}

pub async fn delete_knowledge_entry(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    target: VersionedKnowledgeTarget,
) -> Result<KnowledgeEntry, KnowledgeError> {
    write.begin_operation(Some(caller))?;
    let space_id = direct_space(write, caller, space).await?;
    let entry = locked_entry(write.transaction(), &space_id, &target.target).await?;
    check_version(&entry, target.version)?;
    let now = now_millis()?;
    let mut model = entry.into_active_model();
    model.latest_version = Set(next_version(*model.latest_version.as_ref())?);
    model.deleted_at = Set(Some(now));
    model.updated_at = Set(now);
    let entry = KnowledgeRepository::builder()
        .connection(write.transaction())
        .build()
        .update(model)
        .await?;
    let version = append_version(write.transaction(), caller, &entry, None, None).await?;
    let result = named_entry(write.state(), write.handle(), entry, version).await?;
    write.record(WriteOperation::DeleteKnowledgeEntry, Some(space_id));
    Ok(result)
}

pub async fn read_knowledge_entry(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    target: KnowledgeTarget,
) -> Result<KnowledgeEntry, KnowledgeError> {
    state
        .run_read(async |handle| {
            let space = check_space(state, handle, caller, space, READ_ROLES).await?;
            let entry = KnowledgeRepository::builder()
                .connection(handle.transaction())
                .build()
                .target(&space.space.id, &target, false)
                .await?
                .ok_or(KnowledgeError::NotFound)?;
            if entry.deleted_at.is_some() && caller.agent_name.is_some() {
                return Err(KnowledgeError::NotFound);
            }
            let version = latest(handle.transaction(), &entry).await?;
            named_entry(state, handle, entry, version).await
        })
        .await
}

pub async fn roll_back_knowledge_entry(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    target: VersionedKnowledgeTarget,
    version: i64,
) -> Result<KnowledgeEntry, KnowledgeError> {
    write.begin_operation(Some(caller))?;
    let space_id = direct_space(write, caller, space).await?;
    let entry = KnowledgeRepository::builder()
        .connection(write.transaction())
        .build()
        .target(&space_id, &target.target, true)
        .await?
        .ok_or(KnowledgeError::NotFound)?;
    check_version(&entry, target.version)?;
    let source = KnowledgeVersionRepository::builder()
        .connection(write.transaction())
        .build()
        .find_by_number(&entry.id, version)
        .await?
        .filter(|source| source.kind == "content")
        .ok_or(KnowledgeInputError::RollbackVersion)?;
    let changes = KnowledgeChanges::new(None, source.summary, source.content)?;
    let entry = apply_changes(write.transaction(), caller, entry, changes, None).await?;
    let version = latest(write.transaction(), &entry).await?;
    let result = named_entry(write.state(), write.handle(), entry, version).await?;
    write.record(WriteOperation::RollBackKnowledgeEntry, Some(space_id));
    Ok(result)
}

pub(super) async fn named_entry(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    entry: entry::Model,
    version: version::Model,
) -> Result<KnowledgeEntry, KnowledgeError> {
    let names = state
        .names()
        .names(handle, from_ref(&version.applied_by))
        .await?;
    let applied_by = names
        .get(&version.applied_by)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Person name is missing"))?;
    let kind = KnowledgeVersionKind::try_from(version.kind.as_str())?;
    Ok(KnowledgeEntry {
        key: format_key("KN", entry.number),
        name: entry.name,
        version: version.version,
        kind,
        summary: version.summary,
        content: version.content,
        applied_by,
        applied_at: version.created_at,
    })
}

pub(super) async fn ensure_name_available(
    transaction: &DatabaseTransaction,
    space: &str,
    name: &str,
    own_id: Option<&str>,
) -> Result<(), KnowledgeError> {
    let holder = KnowledgeRepository::builder()
        .connection(transaction)
        .build()
        .by_name(space, &name.to_lowercase())
        .await?;
    if let Some(holder) = holder.filter(|holder| Some(holder.id.as_str()) != own_id) {
        return Err(KnowledgeError::NameTaken {
            deleted: holder.deleted_at.is_some(),
        });
    }
    Ok(())
}

pub(super) async fn insert_entry(
    transaction: &DatabaseTransaction,
    space_id: &str,
    name: &str,
) -> Result<entry::Model, KnowledgeError> {
    let number = SpaceRepository::builder()
        .connection(transaction)
        .build()
        .take_number(space_id, space::Column::NextKnowledgeNumber)
        .await?;
    let now = now_millis()?;
    let model = entry::Model {
        id: primary_key(),
        space_id: space_id.to_owned(),
        number,
        name: name.to_owned(),
        name_key: name.to_lowercase(),
        latest_version: 1,
        deleted_at: None,
        created_at: now,
        updated_at: now,
    };
    let inserted = KnowledgeRepository::builder()
        .connection(transaction)
        .build()
        .insert(model.clone().into_active_model())
        .await?;
    if inserted == 0 {
        ensure_name_available(transaction, space_id, name, None).await?;
        return Err(KnowledgeError::NameTaken { deleted: false });
    }
    Ok(model)
}

pub(super) async fn apply_changes(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    entry: entry::Model,
    changes: KnowledgeChanges,
    person_request_id: Option<&str>,
) -> Result<entry::Model, KnowledgeError> {
    let content =
        if changes.content.is_some() || changes.summary.is_some() || entry.deleted_at.is_some() {
            let previous = KnowledgeVersionRepository::builder()
                .connection(transaction)
                .build()
                .last_content(&entry.id)
                .await?;
            Some((
                changes
                    .summary
                    .or(previous.summary)
                    .ok_or_else(|| anyhow::anyhow!("Content version has no summary"))?,
                changes
                    .content
                    .or(previous.content)
                    .ok_or_else(|| anyhow::anyhow!("Content version has no content"))?,
            ))
        } else {
            None
        };
    let mut model = entry.into_active_model();
    if let Some(name) = changes.name {
        model.name_key = Set(name.to_lowercase());
        model.name = Set(name);
    }
    if content.is_some() {
        model.latest_version = Set(next_version(*model.latest_version.as_ref())?);
        model.deleted_at = Set(None);
    }
    model.updated_at = Set(now_millis()?);
    let entry = KnowledgeRepository::builder()
        .connection(transaction)
        .build()
        .update(model)
        .await
        .map_err(|error| {
            if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
                KnowledgeError::NameTaken { deleted: false }
            } else {
                error.into()
            }
        })?;
    if let Some(content) = content {
        append_version(
            transaction,
            caller,
            &entry,
            Some(content),
            person_request_id,
        )
        .await?;
    }
    Ok(entry)
}

pub(super) async fn append_version(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    entry: &entry::Model,
    content: Option<(String, String)>,
    person_request_id: Option<&str>,
) -> Result<version::Model, KnowledgeError> {
    let (kind, summary, content) = match content {
        Some((summary, content)) => ("content", Some(summary), Some(content)),
        None => ("deletion", None, None),
    };
    Ok(KnowledgeVersionRepository::builder()
        .connection(transaction)
        .build()
        .insert(version::ActiveModel {
            id: Set(primary_key()),
            knowledge_id: Set(entry.id.clone()),
            version: Set(entry.latest_version),
            kind: Set(kind.to_owned()),
            content: Set(content),
            summary: Set(summary),
            applied_by: Set(caller.person_id.clone()),
            person_request_id: Set(person_request_id.map(str::to_owned)),
            created_at: Set(entry.updated_at),
        })
        .await?)
}

fn check_version(entry: &entry::Model, version: i64) -> Result<(), KnowledgeError> {
    if entry.latest_version != version {
        return Err(KnowledgeError::VersionChanged);
    }
    Ok(())
}

pub(super) fn next_version(version: i64) -> Result<i64, KnowledgeError> {
    version
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("Knowledge version exhausted").into())
}

async fn direct_space(
    write: &Write<'_>,
    caller: &Caller,
    space: &str,
) -> Result<String, KnowledgeError> {
    let authorized = check_space(write.state(), write.handle(), caller, space, READ_ROLES).await?;
    caller.person()?;
    if !matches!(authorized.role, SpaceRole::Owner | SpaceRole::Admin) {
        return Err(super::SpaceAccessError::PermissionRequired.into());
    }
    Ok(authorized.space.id)
}

async fn locked_entry(
    transaction: &DatabaseTransaction,
    space: &str,
    target: &KnowledgeTarget,
) -> Result<entry::Model, KnowledgeError> {
    KnowledgeRepository::builder()
        .connection(transaction)
        .build()
        .target(space, target, true)
        .await?
        .filter(|entry| entry.deleted_at.is_none())
        .ok_or(KnowledgeError::NotFound)
}

async fn latest(
    transaction: &DatabaseTransaction,
    entry: &entry::Model,
) -> Result<version::Model, KnowledgeError> {
    Ok(KnowledgeVersionRepository::builder()
        .connection(transaction)
        .build()
        .by_number(&entry.id, entry.latest_version)
        .await?)
}
