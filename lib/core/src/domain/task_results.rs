use lorebay_lib_types::{decode_cursor, encode_cursor, sha256_hex};
use sea_orm::{DatabaseTransaction, Set};
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::entity::{space_task, space_task_result};
use crate::repository::TaskResultRepository;
use crate::{Caller, LibraryState, OperationHandle};

use super::id::primary_key;
use super::tasks::task_by_key;
use super::{Page, PageOptions, TaskError};

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct TaskResultVersion {
    pub version: i64,
    pub result: String,
    pub person: String,
    pub agent_name: String,
    pub created_at: i64,
}

pub async fn read_task_results(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    key: &str,
    options: PageOptions,
) -> Result<Page<TaskResultVersion>, TaskError> {
    state
        .run_read(async |handle| {
            let task = task_by_key(state, handle, caller, space, key).await?;
            result_page(state, handle, caller, &task, options).await
        })
        .await
}

pub(super) async fn record_result(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    task: &space_task::Model,
) -> Result<(), TaskError> {
    TaskResultRepository::builder()
        .connection(transaction)
        .build()
        .insert(space_task_result::ActiveModel {
            id: Set(primary_key()),
            task_id: Set(task.id.clone()),
            version: Set(task.result_version),
            result: Set(task.result.clone()),
            person_id: Set(caller.person_id.clone()),
            agent_name: Set(caller.agent()?.to_owned()),
            created_at: Set(task.last_activity_at),
        })
        .await?;
    Ok(())
}

pub(super) async fn result_page(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    caller: &Caller,
    task: &space_task::Model,
    options: PageOptions,
) -> Result<Page<TaskResultVersion>, TaskError> {
    options.validate()?;
    let scope = format!("task-results-v1:{}", sha256_hex(&task.id));
    let before = options
        .cursor
        .as_ref()
        .map(|cursor| {
            decode_cursor(&scope, caller.person_id(), cursor)
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|version| *version > 0)
                .ok_or(TaskError::InvalidCursor)
        })
        .transpose()?;
    let mut rows = TaskResultRepository::builder()
        .connection(handle.transaction())
        .build()
        .list(&task.id, before, options.page_size + 1)
        .await?;
    let has_more = rows.len() > options.page_size as usize;
    rows.truncate(options.page_size as usize);
    let next_cursor = rows
        .last()
        .filter(|_| has_more)
        .map(|row| encode_cursor(&scope, caller.person_id(), &row.version.to_string()));
    let mut ids: Vec<_> = rows.iter().map(|row| row.person_id.clone()).collect();
    ids.sort_unstable();
    ids.dedup();
    let names = state.names().names(handle, &ids).await?;
    let items = rows
        .into_iter()
        .map(|row| {
            Ok(TaskResultVersion {
                version: row.version,
                result: row.result,
                person: names
                    .get(&row.person_id)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Missing result person name"))?,
                agent_name: row.agent_name,
                created_at: row.created_at,
            })
        })
        .collect::<Result<_, TaskError>>()?;
    Ok(Page { items, next_cursor })
}
