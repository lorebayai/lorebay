use lorebay_lib_types::{decode_cursor, encode_cursor, sha256_hex};
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::repository::{KnowledgeListRequest, KnowledgeListRow, KnowledgeRepository};
use crate::{Caller, LibraryState};

use super::key::format_key;
use super::knowledge_input::validate_knowledge_name;
use super::space_access::{READ_ROLES, check_space};
use super::{KnowledgeError, Page, PageOptions, TaskKeyword};

#[derive(Clone, Debug)]
pub struct KnowledgeFilters {
    prefix: Option<String>,
    keyword: Option<TaskKeyword>,
}

impl KnowledgeFilters {
    pub fn new(prefix: Option<String>, keyword: Option<String>) -> Result<Self, KnowledgeError> {
        if prefix.is_some() && keyword.is_some() {
            return Err(KnowledgeError::InvalidFilter);
        }
        let prefix = prefix
            .map(|prefix| {
                let prefix = prefix.strip_suffix('/').unwrap_or(&prefix);
                validate_knowledge_name(prefix).map_err(|_| KnowledgeError::InvalidFilter)?;
                Ok::<_, KnowledgeError>(prefix.to_lowercase())
            })
            .transpose()?;
        let keyword = keyword
            .map(|keyword| {
                TaskKeyword::try_from(keyword).map_err(|_| KnowledgeError::InvalidFilter)
            })
            .transpose()?;
        Ok(Self { prefix, keyword })
    }
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct KnowledgeListItem {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

pub async fn list_knowledge(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    options: PageOptions,
    filters: KnowledgeFilters,
) -> Result<Page<KnowledgeListItem>, KnowledgeError> {
    options.validate()?;
    let scope = serde_json::to_string(&(
        space,
        &filters.prefix,
        filters.keyword.as_ref().map(TaskKeyword::as_str),
    ))
    .map_err(anyhow::Error::from)?;
    let version = format!("knowledge-v1:{}", sha256_hex(&scope));
    let after = options
        .cursor
        .as_ref()
        .map(|cursor| {
            decode_cursor(&version, caller.person_id(), cursor).ok_or(KnowledgeError::InvalidCursor)
        })
        .transpose()?;
    state
        .run_read(async |handle| {
            let space = check_space(state, handle, caller, space, READ_ROLES).await?;
            let mut rows = KnowledgeRepository::builder()
                .connection(handle.transaction())
                .build()
                .list(KnowledgeListRequest {
                    space: &space.space.id,
                    prefix: filters.prefix.as_deref(),
                    keyword: filters.keyword.as_ref().map(TaskKeyword::as_str),
                    after: after.as_deref(),
                    limit: options.page_size + 1,
                })
                .await?;
            let has_more = rows.len() > options.page_size as usize;
            rows.truncate(options.page_size as usize);
            let next_cursor = rows
                .last()
                .filter(|_| has_more)
                .map(|row| encode_cursor(&version, caller.person_id(), &row.position));
            let level = filters
                .prefix
                .as_ref()
                .map_or(0, |prefix| prefix.split('/').count());
            Ok(Page {
                items: rows
                    .into_iter()
                    .map(|row| listed_item(row, level, filters.keyword.is_some()))
                    .collect(),
                next_cursor,
            })
        })
        .await
}

fn listed_item(row: KnowledgeListRow, level: usize, search: bool) -> KnowledgeListItem {
    let levels: Vec<_> = row.name.split('/').collect();
    if !search && levels.len() > level + 1 {
        KnowledgeListItem {
            name: format!("{}/", levels[..=level].join("/")),
            key: None,
            summary: None,
        }
    } else {
        KnowledgeListItem {
            name: row.name,
            key: Some(format_key("KN", row.number)),
            summary: Some(row.summary),
        }
    }
}
