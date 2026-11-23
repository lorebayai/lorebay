use std::collections::HashMap;

use lorebay_lib_types::{decode_cursor, encode_cursor, sha256_hex};
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::entity::space_knowledge;
use crate::repository::{
    DeletedKnowledgeRow, KnowledgeRepository, KnowledgeVersionRepository, KnowledgeVersionRow,
};
use crate::{Caller, LibraryState, OperationHandle};

use super::key::format_key;
use super::knowledge::named_entry;
use super::space_access::{READ_ROLES, check_space};
use super::{
    KnowledgeEntry, KnowledgeError, KnowledgeTarget, KnowledgeVersionKind, Page, PageOptions,
};

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct KnowledgeVersion {
    pub version: i64,
    pub kind: KnowledgeVersionKind,
    pub applied_by: String,
    pub applied_at: i64,
    pub request_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct DeletedKnowledgeEntry {
    pub key: String,
    pub name: String,
    pub deleted_by: String,
    pub deleted_at: i64,
}

pub async fn list_knowledge_versions(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    target: KnowledgeTarget,
    options: PageOptions,
) -> Result<Page<KnowledgeVersion>, KnowledgeError> {
    options.validate()?;
    state
        .run_read(async |handle| {
            let entry = history_entry(state, handle, caller, space, &target).await?;
            let version = format!("knowledge-versions-v1:{}", sha256_hex(&entry.id));
            let before = cursor_number(&options, caller, &version)?;
            let mut rows = KnowledgeVersionRepository::builder()
                .connection(handle.transaction())
                .build()
                .list(&entry.id, before, options.page_size + 1)
                .await?;
            let has_more = rows.len() > options.page_size as usize;
            rows.truncate(options.page_size as usize);
            let next_cursor = rows
                .last()
                .filter(|_| has_more)
                .map(|row| encode_cursor(&version, caller.person_id(), &row.version.to_string()));
            let names = person_names(
                state,
                handle,
                rows.iter().map(|row| row.applied_by.clone()).collect(),
            )
            .await?;
            Ok(Page {
                items: rows
                    .into_iter()
                    .map(|row| named_version(row, &names))
                    .collect::<Result<_, _>>()?,
                next_cursor,
            })
        })
        .await
}

pub async fn read_knowledge_version(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    target: KnowledgeTarget,
    version: i64,
) -> Result<KnowledgeEntry, KnowledgeError> {
    state
        .run_read(async |handle| {
            let entry = history_entry(state, handle, caller, space, &target).await?;
            let version = KnowledgeVersionRepository::builder()
                .connection(handle.transaction())
                .build()
                .find_by_number(&entry.id, version)
                .await?
                .ok_or(KnowledgeError::VersionNotFound)?;
            named_entry(state, handle, entry, version).await
        })
        .await
}

pub async fn list_deleted_knowledge(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    options: PageOptions,
) -> Result<Page<DeletedKnowledgeEntry>, KnowledgeError> {
    options.validate()?;
    state
        .run_read(async |handle| {
            let space = history_space(state, handle, caller, space).await?;
            let version = format!("knowledge-deleted-v1:{}", sha256_hex(&space));
            let after = cursor_number(&options, caller, &version)?;
            let mut rows = KnowledgeRepository::builder()
                .connection(handle.transaction())
                .build()
                .list_deleted(&space, after, options.page_size + 1)
                .await?;
            let has_more = rows.len() > options.page_size as usize;
            rows.truncate(options.page_size as usize);
            let next_cursor = rows
                .last()
                .filter(|_| has_more)
                .map(|row| encode_cursor(&version, caller.person_id(), &row.number.to_string()));
            let names = person_names(
                state,
                handle,
                rows.iter().map(|row| row.applied_by.clone()).collect(),
            )
            .await?;
            Ok(Page {
                items: rows
                    .into_iter()
                    .map(|row| deleted_entry(row, &names))
                    .collect::<Result<_, _>>()?,
                next_cursor,
            })
        })
        .await
}

async fn history_space(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    caller: &Caller,
    space: &str,
) -> Result<String, KnowledgeError> {
    let space = check_space(state, handle, caller, space, READ_ROLES).await?;
    caller.person()?;
    Ok(space.space.id)
}

async fn history_entry(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    caller: &Caller,
    space: &str,
    target: &KnowledgeTarget,
) -> Result<space_knowledge::Model, KnowledgeError> {
    let space = history_space(state, handle, caller, space).await?;
    KnowledgeRepository::builder()
        .connection(handle.transaction())
        .build()
        .target(&space, target, false)
        .await?
        .ok_or(KnowledgeError::NotFound)
}

async fn person_names(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    mut people: Vec<String>,
) -> Result<HashMap<String, String>, KnowledgeError> {
    people.sort();
    people.dedup();
    Ok(state.names().names(handle, &people).await?)
}

fn cursor_number(
    options: &PageOptions,
    caller: &Caller,
    version: &str,
) -> Result<Option<i64>, KnowledgeError> {
    options
        .cursor
        .as_ref()
        .map(|cursor| {
            decode_cursor(version, caller.person_id(), cursor)
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|number| *number > 0)
                .ok_or(KnowledgeError::InvalidCursor)
        })
        .transpose()
}

fn named_version(
    row: KnowledgeVersionRow,
    names: &HashMap<String, String>,
) -> Result<KnowledgeVersion, KnowledgeError> {
    Ok(KnowledgeVersion {
        version: row.version,
        kind: KnowledgeVersionKind::try_from(row.kind.as_str())?,
        applied_by: person_name(names, &row.applied_by)?,
        applied_at: row.created_at,
        request_key: row
            .person_request_number
            .map(|number| format_key("RQ", number)),
    })
}

fn deleted_entry(
    row: DeletedKnowledgeRow,
    names: &HashMap<String, String>,
) -> Result<DeletedKnowledgeEntry, KnowledgeError> {
    Ok(DeletedKnowledgeEntry {
        key: format_key("KN", row.number),
        name: row.name,
        deleted_by: person_name(names, &row.applied_by)?,
        deleted_at: row.deleted_at,
    })
}

fn person_name(names: &HashMap<String, String>, person: &str) -> Result<String, KnowledgeError> {
    names
        .get(person)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Person name is missing").into())
}
