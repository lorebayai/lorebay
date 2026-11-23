use std::collections::HashMap;
use std::slice::from_ref;

use lorebay_lib_types::Secret;
use lorebay_lib_types::{decode_cursor, encode_cursor};
use sea_orm::{DatabaseTransaction, DbErr, IntoActiveModel, Set};
use serde::Serialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::{space, space_checkpoint as checkpoint, space_task as task};
use crate::repository::{
    AgentFileRepository, CheckpointRepository, SpaceRepository, TaskAfter, TaskListFilter,
    TaskRepository,
};
use crate::{Caller, LibraryState, OperationHandle, Write, WriteOperation};

use super::id::{primary_key, random_task_id};
use super::key::{format_key, parse_key};
use super::pagination::{Page, PageOptions, PageSizeError};
use super::space_access::{READ_ROLES, SpaceAccessError, WRITE_ROLES, check_space};
use super::task_access::{TaskAccessError, TaskAction, TaskCheck, check_task};
use super::time::now_millis;
use super::{
    AgentFile, CallerError, CheckpointContent, TaskFilterError, TaskFilters, TaskResult, TaskTitle,
};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TaskError {
    #[error(transparent)]
    Access(#[from] TaskAccessError),
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error("Checkpoint not found")]
    CheckpointNotFound,
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Filter(#[from] TaskFilterError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error(transparent)]
    InvalidPageSize(#[from] PageSizeError),
    #[error(transparent)]
    NoChanges(#[from] TaskChangeError),
    #[error("Task not found")]
    NotFound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum TaskStatus {
    #[serde(rename = "ended")]
    Ended,
    #[serde(rename = "in progress")]
    InProgress,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct CreatedTask {
    pub task_title: String,
    pub key: String,
    pub task_id: Secret<String>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct Checkpoint {
    pub time: i64,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct CheckpointAdded {
    pub time: i64,
    pub task_title: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct Task {
    pub key: String,
    pub task_id: Secret<String>,
    pub title: String,
    pub status: TaskStatus,
    pub person: String,
    pub agent_name: String,
    pub result: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub last_activity_at: i64,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct TaskChanges {
    pub title: Option<TaskTitle>,
    pub status: Option<TaskStatus>,
    pub result: Option<TaskResult>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct TaskRead {
    pub task: Task,
    pub scratchpad: Option<Page<Checkpoint>>,
    pub files: Vec<AgentFile>,
}

#[derive(Debug, Error)]
#[error("Provide a title, status or result.")]
pub struct TaskChangeError;

impl TaskChangeError {
    pub const FIELD: &str = "body";
    pub const FIELD_CODE: &str = "required";
}

#[derive(Debug, Error)]
#[error("Expected in progress or ended.")]
pub struct TaskStatusError;

impl TaskStatusError {
    pub const FIELD: &str = "status";
    pub const FIELD_CODE: &str = "invalid";
}

impl TaskChanges {
    pub fn validate(&self) -> Result<(), TaskChangeError> {
        if self.title.is_none() && self.status.is_none() && self.result.is_none() {
            return Err(TaskChangeError);
        }
        Ok(())
    }
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ended => "ended",
            Self::InProgress => "in progress",
        }
    }
}

impl TryFrom<&str> for TaskStatus {
    type Error = TaskStatusError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "ended" => Ok(Self::Ended),
            "in progress" => Ok(Self::InProgress),
            _ => Err(TaskStatusError),
        }
    }
}

impl TryFrom<(task::Model, String)> for Task {
    type Error = DbErr;

    fn try_from((task, person): (task::Model, String)) -> Result<Self, Self::Error> {
        Ok(Self::builder()
            .key(format_key("TK", task.number))
            .task_id(Secret::from(task.random_id))
            .title(task.title)
            .status(
                TaskStatus::try_from(task.status.as_str())
                    .map_err(|_| DbErr::Type("Invalid task status".into()))?,
            )
            .person(person)
            .agent_name(task.agent_name)
            .result(task.result)
            .started_at(task.started_at)
            .ended_at(task.ended_at)
            .last_activity_at(task.last_activity_at)
            .build())
    }
}

impl From<checkpoint::Model> for Checkpoint {
    fn from(checkpoint: checkpoint::Model) -> Self {
        Self::builder()
            .time(checkpoint.created_at)
            .content(checkpoint.content)
            .build()
    }
}

impl From<SpaceAccessError> for TaskError {
    fn from(error: SpaceAccessError) -> Self {
        Self::Access(error.into())
    }
}

impl TaskError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Access(error) => error.code(),
            Self::Caller(error) => error.code(),
            Self::Database(_) | Self::Internal(_) => "server.internal",
            Self::Filter(_) | Self::InvalidCursor | Self::InvalidPageSize(_) => {
                "request.invalid_query"
            }
            Self::NoChanges(_) => "request.invalid_body",
            Self::NotFound => "task.not_found",
            Self::CheckpointNotFound => "checkpoint.not_found",
        }
    }
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct TaskSummary {
    pub key: String,
    pub task_id: Secret<String>,
    pub title: String,
    pub status: TaskStatus,
    pub person: String,
    pub agent_name: String,
    pub last_activity_at: i64,
}

impl From<Task> for TaskSummary {
    fn from(task: Task) -> Self {
        Self {
            key: task.key,
            task_id: task.task_id,
            title: task.title,
            status: task.status,
            person: task.person,
            agent_name: task.agent_name,
            last_activity_at: task.last_activity_at,
        }
    }
}

pub async fn create_task(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    title: TaskTitle,
) -> Result<CreatedTask, TaskError> {
    write.begin_operation(Some(caller))?;
    let agent = caller.agent()?;
    let authorized = check_space(write.state(), write.handle(), caller, space, WRITE_ROLES).await?;
    let transaction = write.transaction();
    let space_id = authorized.space.id;
    let number = SpaceRepository::builder()
        .connection(transaction)
        .build()
        .take_number(&space_id, space::Column::NextTaskNumber)
        .await?;
    let now = now_millis()?;
    let task_id = random_task_id().map_err(anyhow::Error::from)?;
    let task = TaskRepository::builder()
        .connection(transaction)
        .build()
        .insert(task::ActiveModel {
            id: Set(primary_key()),
            space_id: Set(space_id.clone()),
            number: Set(number),
            random_id: Set(task_id.clone()),
            person_id: Set(caller.person_id.clone()),
            agent_name: Set(agent.to_owned()),
            title: Set(title.into()),
            status: Set(TaskStatus::InProgress.as_str().to_owned()),
            result: Set(String::new()),
            started_at: Set(now),
            ended_at: Set(None),
            last_activity_at: Set(now),
        })
        .await?;
    write.record(WriteOperation::CreateTask, Some(space_id));
    Ok(CreatedTask {
        task_title: task.title,
        key: format_key("TK", number),
        task_id: Secret::from(task_id),
    })
}

pub async fn add_checkpoint(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    content: CheckpointContent,
) -> Result<CheckpointAdded, TaskError> {
    write.begin_operation(Some(caller))?;
    caller.agent()?;
    let task = check_task(
        write.state(),
        write.handle(),
        caller,
        target.check(TaskAction::AppendCheckpoint),
    )
    .await?;
    let transaction = write.transaction();
    let checkpoints = CheckpointRepository::builder()
        .connection(transaction)
        .build();
    let latest = checkpoints
        .latest_time(&task.id)
        .await?
        .unwrap_or(task.last_activity_at)
        .max(task.last_activity_at);
    let time = now_millis()?.max(
        latest
            .checked_add(1)
            .ok_or_else(|| DbErr::Type("Checkpoint times exhausted".into()))?,
    );
    checkpoints
        .insert(checkpoint::ActiveModel {
            id: Set(primary_key()),
            task_id: Set(task.id.clone()),
            content: Set(content.into()),
            created_at: Set(time),
        })
        .await?;
    let title = task.title.clone();
    let space_id = task.space_id.clone();
    let mut task = task.into_active_model();
    task.last_activity_at = Set(time);
    TaskRepository::builder()
        .connection(transaction)
        .build()
        .update(task)
        .await?;
    write.record(WriteOperation::AddCheckpoint, Some(space_id));
    Ok(CheckpointAdded {
        time,
        task_title: title,
    })
}

pub async fn change_task(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    changes: TaskChanges,
) -> Result<TaskRead, TaskError> {
    write.begin_operation(Some(caller))?;
    changes.validate()?;
    let action = if changes.title.is_none()
        && changes.result.is_none()
        && changes.status == Some(TaskStatus::Ended)
    {
        TaskAction::Close
    } else {
        TaskAction::Change
    };
    let handle = write.handle();
    let task = check_task(write.state(), handle, caller, target.check(action)).await?;
    let model = changed_task(task, changes)?;
    let task = TaskRepository::builder()
        .connection(handle.transaction())
        .build()
        .update(model)
        .await?;
    let space_id = task.space_id.clone();
    let result = full_task(write.state(), handle, task, None).await?;
    write.record(WriteOperation::ChangeTask, Some(space_id));
    Ok(result)
}

pub async fn delete_task(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
) -> Result<(), TaskError> {
    write.begin_operation(Some(caller))?;
    caller.person()?;
    let task = check_task(
        write.state(),
        write.handle(),
        caller,
        target.check(TaskAction::Delete),
    )
    .await?;
    let transaction = write.transaction();
    let locations = AgentFileRepository::builder()
        .connection(transaction)
        .build()
        .delete_task_agent_files(&task.id)
        .await?;
    TaskRepository::builder()
        .connection(transaction)
        .build()
        .delete(&task.id)
        .await?;
    write.delete.extend(locations);
    write.record(WriteOperation::DeleteTask, Some(task.space_id));
    Ok(())
}

pub async fn delete_checkpoint(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    time: i64,
) -> Result<(), TaskError> {
    write.begin_operation(Some(caller))?;
    caller.person()?;
    let task = check_task(
        write.state(),
        write.handle(),
        caller,
        target.check(TaskAction::DeleteCheckpoint),
    )
    .await?;
    let deleted = CheckpointRepository::builder()
        .connection(write.transaction())
        .build()
        .delete_by_time(&task.id, time)
        .await?;
    if deleted == 0 {
        return Err(TaskError::CheckpointNotFound);
    }
    write.record(WriteOperation::DeleteCheckpoint, Some(task.space_id));
    Ok(())
}

pub async fn list_tasks(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    options: PageOptions,
    filters: TaskFilters,
) -> Result<Page<TaskSummary>, TaskError> {
    filters.validate()?;
    options.validate()?;
    state
        .run_read(async |handle| {
            let authorized = check_space(state, handle, caller, space, READ_ROLES).await?;
            let version = filters.cursor_version(space).map_err(anyhow::Error::from)?;
            let after = task_position(&options, &version, caller.person_id())?;
            let person_id = match filters.person.as_deref() {
                Some(name) => match state.names().person_id(handle, name).await? {
                    Some(id) => Some(id),
                    None => {
                        return Ok(Page {
                            items: Vec::new(),
                            next_cursor: None,
                        });
                    }
                },
                None => filters.own_open.then(|| caller.person_id.clone()),
            };
            let filter = TaskListFilter {
                person_id: person_id.as_deref(),
                agent: filters.agent.as_deref(),
                status: filters
                    .own_open
                    .then_some(TaskStatus::InProgress)
                    .or(filters.status)
                    .map(TaskStatus::as_str),
                keyword: filters.keyword.as_ref().map(|keyword| keyword.as_str()),
            };
            let mut records = TaskRepository::builder()
                .connection(handle.transaction())
                .build()
                .list(
                    &authorized.space.id,
                    &filter,
                    after.as_ref(),
                    options.page_size + 1,
                )
                .await?;
            let has_more = records.len() as u64 > options.page_size;
            records.truncate(options.page_size as usize);
            let next_cursor = records.last().filter(|_| has_more).map(|task| {
                encode_cursor(
                    &version,
                    caller.person_id(),
                    &format!("{}:{}", task.last_activity_at, task.number),
                )
            });
            let people = names_for(state, handle, &records).await?;
            let items = records
                .into_iter()
                .map(|task| named_task(task, &people).map(TaskSummary::from))
                .collect::<Result<_, _>>()?;
            Ok(Page { items, next_cursor })
        })
        .await
}

pub async fn read_task(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    key: &str,
    scratchpad: Option<PageOptions>,
) -> Result<TaskRead, TaskError> {
    state
        .run_read(async |handle| {
            let task = task_by_key(state, handle, caller, space, key).await?;
            let checkpoints = match scratchpad {
                Some(options) => Some(
                    checkpoint_page(handle.transaction(), caller, &task, space, options).await?,
                ),
                None => None,
            };
            full_task(state, handle, task, checkpoints).await
        })
        .await
}

pub async fn read_scratchpad(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    key: &str,
    options: PageOptions,
) -> Result<Page<Checkpoint>, TaskError> {
    state
        .run_read(async |handle| {
            let task = task_by_key(state, handle, caller, space, key).await?;
            checkpoint_page(handle.transaction(), caller, &task, space, options).await
        })
        .await
}

#[derive(Clone, Copy, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct TaskTarget<'a> {
    pub path_space: Option<&'a str>,
    pub task_id: &'a str,
}

impl<'a> TaskTarget<'a> {
    pub(super) fn check(self, action: TaskAction) -> TaskCheck<'a> {
        TaskCheck {
            path_space: self.path_space,
            task_id: self.task_id,
            action,
        }
    }
}

fn changed_task(task: task::Model, changes: TaskChanges) -> Result<task::ActiveModel, TaskError> {
    let status = changes
        .status
        .filter(|status| status.as_str() != task.status);
    let last_activity_at = task.last_activity_at;
    let mut model = task.into_active_model();
    if let Some(title) = changes.title {
        model.title = Set(title.into());
    }
    let now = now_millis()?;
    if let Some(status) = status {
        model.status = Set(status.as_str().to_owned());
        model.ended_at = Set((status == TaskStatus::Ended).then_some(now));
    }
    if let Some(result) = changes.result {
        model.result = Set(result.into());
        model.last_activity_at = Set(now.max(last_activity_at));
    }
    Ok(model)
}

async fn names_for(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    tasks: &[task::Model],
) -> anyhow::Result<HashMap<String, String>> {
    let mut ids: Vec<String> = tasks.iter().map(|task| task.person_id.clone()).collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    state.names().names(handle, &ids).await
}

fn named_task(task: task::Model, names: &HashMap<String, String>) -> Result<Task, DbErr> {
    let name = names
        .get(&task.person_id)
        .cloned()
        .ok_or_else(|| DbErr::Type("Missing task person name".into()))?;
    Task::try_from((task, name))
}

async fn full_task(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    task: task::Model,
    checkpoints: Option<Page<Checkpoint>>,
) -> Result<TaskRead, TaskError> {
    let names = names_for(state, handle, from_ref(&task)).await?;
    let files = completed_agent_files(handle.transaction(), &task.id).await?;
    Ok(TaskRead {
        task: named_task(task, &names)?,
        scratchpad: checkpoints,
        files,
    })
}

async fn task_by_key(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    caller: &Caller,
    space: &str,
    key: &str,
) -> Result<task::Model, TaskError> {
    let authorized = check_space(state, handle, caller, space, READ_ROLES).await?;
    let number = parse_key("TK", key).ok_or(TaskError::NotFound)?;
    TaskRepository::builder()
        .connection(handle.transaction())
        .build()
        .by_number(&authorized.space.id, number)
        .await?
        .ok_or(TaskError::NotFound)
}

async fn completed_agent_files(
    transaction: &DatabaseTransaction,
    task_id: &str,
) -> Result<Vec<AgentFile>, DbErr> {
    AgentFileRepository::builder()
        .connection(transaction)
        .build()
        .completed(task_id)
        .await?
        .into_iter()
        .map(AgentFile::try_from)
        .collect()
}

async fn checkpoint_page(
    transaction: &DatabaseTransaction,
    caller: &Caller,
    task: &task::Model,
    space: &str,
    options: PageOptions,
) -> Result<Page<Checkpoint>, TaskError> {
    options.validate()?;
    let version = format!("checkpoints-v1:{space}:{}", task.number);
    let after = options
        .cursor
        .map(|cursor| {
            decode_cursor(&version, caller.person_id(), &cursor)
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|time| *time >= 0)
                .ok_or(TaskError::InvalidCursor)
        })
        .transpose()?;
    let mut records = CheckpointRepository::builder()
        .connection(transaction)
        .build()
        .list(&task.id, after, options.page_size + 1)
        .await?;
    let has_more = records.len() as u64 > options.page_size;
    records.truncate(options.page_size as usize);
    let next_cursor = records.last().filter(|_| has_more).map(|checkpoint| {
        encode_cursor(
            &version,
            caller.person_id(),
            &checkpoint.created_at.to_string(),
        )
    });
    Ok(Page::builder()
        .items(
            records
                .into_iter()
                .map(Checkpoint::from)
                .collect::<Vec<_>>(),
        )
        .next_cursor(next_cursor)
        .build())
}

fn task_position(
    options: &PageOptions,
    version: &str,
    person_id: &str,
) -> Result<Option<TaskAfter>, TaskError> {
    options
        .cursor
        .as_ref()
        .map(|cursor| {
            let value =
                decode_cursor(version, person_id, cursor).ok_or(TaskError::InvalidCursor)?;
            let (time, number) = value.split_once(':').ok_or(TaskError::InvalidCursor)?;
            let time = time.parse::<i64>().map_err(|_| TaskError::InvalidCursor)?;
            let number = number
                .parse::<i64>()
                .map_err(|_| TaskError::InvalidCursor)?;
            if time < 0 || number <= 0 {
                return Err(TaskError::InvalidCursor);
            }
            Ok(TaskAfter::builder()
                .last_activity_at(time)
                .number(number)
                .build())
        })
        .transpose()
}
