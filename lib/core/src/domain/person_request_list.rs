use std::collections::{HashMap, HashSet};

use lorebay_lib_types::{decode_cursor, encode_cursor, sha256_hex};
use sea_orm::DatabaseTransaction;
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::repository::{
    KnowledgeRepository, PersonRequestAfter, PersonRequestListRequest, PersonRequestRepository,
    PersonRequestRow,
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
    pub reviser_agent_name: Option<String>,
    pub revised_by: Option<String>,
    pub revised_at: Option<i64>,
    pub revision: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outdated: Option<bool>,
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
                .flat_map(|row| {
                    [
                        Some(row.model.requested_by.clone()),
                        row.model.revised_by.clone(),
                    ]
                    .into_iter()
                    .flatten()
                })
                .collect::<Vec<_>>();
            let names = state.names().names(handle, &ids).await?;
            Ok(Page {
                items: named_items(&rows, &names, handle.transaction()).await?,
                next_cursor,
            })
        })
        .await
}

pub(super) async fn named_items(
    rows: &[PersonRequestRow],
    names: &HashMap<String, String>,
    transaction: &DatabaseTransaction,
) -> Result<Vec<PersonRequestListItem>, PersonRequestError> {
    let Some(first) = rows.first() else {
        return Ok(Vec::new());
    };
    let mut proposed_names: Vec<_> = rows
        .iter()
        .filter(|row| row.model.action == "create" && row.model.status == "pending")
        .filter_map(|row| row.model.new_name.as_ref().map(|name| name.to_lowercase()))
        .collect();
    proposed_names.sort();
    proposed_names.dedup();
    let occupied: HashSet<_> = KnowledgeRepository::builder()
        .connection(transaction)
        .build()
        .by_names(&first.model.space_id, proposed_names)
        .await?
        .into_iter()
        .map(|entry| entry.name_key)
        .collect();
    rows.iter()
        .map(|row| named_item(row, names, &occupied))
        .collect()
}

pub(super) fn named_item(
    row: &PersonRequestRow,
    names: &HashMap<String, String>,
    occupied: &HashSet<String>,
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
        reviser_agent_name: model.reviser_agent_name.clone(),
        revised_by: model
            .revised_by
            .as_ref()
            .map(|id| {
                names
                    .get(id)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Person name is missing"))
            })
            .transpose()?,
        revised_at: model.revised_at,
        revision: model.revision,
        outdated: if model.status != "pending" {
            None
        } else if model.action == "create" {
            Some(
                model
                    .new_name
                    .as_ref()
                    .is_some_and(|name| occupied.contains(&name.to_lowercase())),
            )
        } else {
            Some(row.latest_version != model.base_version || row.target_name != model.base_name)
        },
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
