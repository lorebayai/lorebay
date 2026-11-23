use std::collections::HashMap;

use lorebay_lib_types::{decode_cursor, encode_cursor, sha256_hex};
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::repository::{
    PersonRequestAfter, PersonRequestListRequest, PersonRequestRepository, PersonRequestRow,
};
use crate::{Caller, LibraryState};

use super::key::format_key;
use super::space_access::{READ_ROLES, check_space};
use super::{
    Page, PageOptions, PersonRequestAction, PersonRequestError, PersonRequestFilters,
    PersonRequestStatus, TaskKeyword,
};

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct PersonRequestEntry {
    pub key: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct PersonRequestListItem {
    pub key: String,
    pub action: PersonRequestAction,
    pub status: PersonRequestStatus,
    pub requested_by: String,
    pub requester_agent_name: Option<String>,
    pub target: Option<PersonRequestEntry>,
    pub new_name: Option<String>,
    pub new_summary: Option<String>,
    pub created_at: i64,
}

pub async fn list_person_requests(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    options: PageOptions,
    filters: PersonRequestFilters,
) -> Result<Page<PersonRequestListItem>, PersonRequestError> {
    options.validate()?;
    let scope = serde_json::to_string(&(
        space,
        filters.status,
        filters.keyword.as_ref().map(TaskKeyword::as_str),
    ))
    .map_err(anyhow::Error::from)?;
    let version = format!("requests-v1:{}", sha256_hex(&scope));
    let after = options
        .cursor
        .as_ref()
        .map(|cursor| {
            decode_cursor(&version, caller.person_id(), cursor)
                .and_then(|value| serde_json::from_str::<PersonRequestAfter>(&value).ok())
                .filter(|value| value.number > 0)
                .ok_or(PersonRequestError::InvalidCursor)
        })
        .transpose()?;
    state
        .run_read(async |handle| {
            let space = check_space(state, handle, caller, space, READ_ROLES).await?;
            let mut rows = PersonRequestRepository::builder()
                .connection(handle.transaction())
                .build()
                .list(PersonRequestListRequest {
                    space: &space.space.id,
                    status: filters.status.map(PersonRequestStatus::as_str),
                    keyword: filters.keyword.as_ref().map(TaskKeyword::as_str),
                    after,
                    limit: options.page_size + 1,
                })
                .await?;
            let more = rows.len() > options.page_size as usize;
            rows.truncate(options.page_size as usize);
            let next_cursor = rows
                .last()
                .filter(|_| more)
                .map(|row| cursor(&version, caller, row))
                .transpose()?;
            let ids = rows
                .iter()
                .map(|row| row.model.requested_by.clone())
                .collect::<Vec<_>>();
            let names = state.names().names(handle, &ids).await?;
            Ok(Page {
                items: named_items(&rows, &names)?,
                next_cursor,
            })
        })
        .await
}

pub(super) fn named_items(
    rows: &[PersonRequestRow],
    names: &HashMap<String, String>,
) -> Result<Vec<PersonRequestListItem>, PersonRequestError> {
    rows.iter().map(|row| named_item(row, names)).collect()
}

pub(super) fn named_item(
    row: &PersonRequestRow,
    names: &HashMap<String, String>,
) -> Result<PersonRequestListItem, PersonRequestError> {
    let model = &row.model;
    let action = match model.action.as_str() {
        "create" => PersonRequestAction::Create,
        "change" => PersonRequestAction::Change,
        "delete" => PersonRequestAction::Delete,
        _ => return Err(anyhow::anyhow!("Invalid request action").into()),
    };
    Ok(PersonRequestListItem {
        key: format_key("RQ", model.number),
        action,
        status: PersonRequestStatus::try_from(model.status.as_str())?,
        requested_by: names
            .get(&model.requested_by)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Person name is missing"))?,
        requester_agent_name: model.requester_agent_name.clone(),
        target: row
            .target_name
            .clone()
            .zip(row.target_number)
            .map(|(name, number)| PersonRequestEntry {
                name,
                key: format_key("KN", number),
            }),
        new_name: model.new_name.clone(),
        new_summary: model.new_summary.clone(),
        created_at: model.created_at,
    })
}

fn cursor(
    version: &str,
    caller: &Caller,
    row: &PersonRequestRow,
) -> Result<String, PersonRequestError> {
    let position = serde_json::to_string(&PersonRequestAfter {
        created_at: row.model.created_at,
        number: row.model.number,
    })
    .map_err(anyhow::Error::from)?;
    Ok(encode_cursor(version, caller.person_id(), &position))
}
