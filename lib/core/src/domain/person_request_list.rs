use std::collections::HashMap;

use lorebay_lib_types::{decode_cursor, encode_cursor, sha256_hex};
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::repository::{ProposalAfter, ProposalListRequest, ProposalRepository, ProposalRow};
use crate::{Caller, LibraryState};

use super::key::format_key;
use super::space_access::{READ_ROLES, check_space};
use super::{
    Page, PageOptions, ProposalError, ProposalFilters, ProposalKind, ProposalStatus, TaskKeyword,
};

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct ProposalEntry {
    pub key: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct ProposalListItem {
    pub key: String,
    pub kind: ProposalKind,
    pub status: ProposalStatus,
    pub proposed_by: String,
    pub proposer_agent_name: Option<String>,
    pub target: Option<ProposalEntry>,
    pub new_name: Option<String>,
    pub new_summary: Option<String>,
    pub created_at: i64,
}

pub async fn list_proposals(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    options: PageOptions,
    filters: ProposalFilters,
) -> Result<Page<ProposalListItem>, ProposalError> {
    options.validate()?;
    let scope = serde_json::to_string(&(
        space,
        filters.status,
        filters.keyword.as_ref().map(TaskKeyword::as_str),
    ))
    .map_err(anyhow::Error::from)?;
    let version = format!("proposals-v1:{}", sha256_hex(&scope));
    let after = options
        .cursor
        .as_ref()
        .map(|cursor| {
            decode_cursor(&version, caller.person_id(), cursor)
                .and_then(|value| serde_json::from_str::<ProposalAfter>(&value).ok())
                .filter(|value| value.number > 0)
                .ok_or(ProposalError::InvalidCursor)
        })
        .transpose()?;
    state
        .run_read(async |handle| {
            let space = check_space(state, handle, caller, space, READ_ROLES).await?;
            let mut rows = ProposalRepository::builder()
                .connection(handle.transaction())
                .build()
                .list(ProposalListRequest {
                    space: &space.space.id,
                    status: filters.status.map(ProposalStatus::as_str),
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
                .map(|row| {
                    let position = serde_json::to_string(&ProposalAfter {
                        created_at: row.model.created_at,
                        number: row.model.number,
                    })
                    .map_err(anyhow::Error::from)?;
                    Ok::<_, ProposalError>(encode_cursor(&version, caller.person_id(), &position))
                })
                .transpose()?;
            let ids = rows
                .iter()
                .map(|row| row.model.proposed_by.clone())
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
    rows: &[ProposalRow],
    names: &HashMap<String, String>,
) -> Result<Vec<ProposalListItem>, ProposalError> {
    rows.iter().map(|row| named_item(row, names)).collect()
}

pub(super) fn named_item(
    row: &ProposalRow,
    names: &HashMap<String, String>,
) -> Result<ProposalListItem, ProposalError> {
    let model = &row.model;
    let kind = match model.kind.as_str() {
        "create" => ProposalKind::Create,
        "change" => ProposalKind::Change,
        "delete" => ProposalKind::Delete,
        _ => return Err(anyhow::anyhow!("Invalid proposal kind").into()),
    };
    Ok(ProposalListItem {
        key: format_key("PP", model.number),
        kind,
        status: ProposalStatus::try_from(model.status.as_str())?,
        proposed_by: names
            .get(&model.proposed_by)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Person name is missing"))?,
        proposer_agent_name: model.proposer_agent_name.clone(),
        target: row
            .target_name
            .clone()
            .zip(row.target_number)
            .map(|(name, number)| ProposalEntry {
                name,
                key: format_key("KN", number),
            }),
        new_name: model.new_name.clone(),
        new_summary: model.new_summary.clone(),
        created_at: model.created_at,
    })
}
