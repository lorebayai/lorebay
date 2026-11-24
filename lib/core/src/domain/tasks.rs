use std::collections::HashMap;
use std::slice::from_ref;

use lorebay_lib_types::Secret;
use lorebay_lib_types::{decode_cursor, encode_cursor};
use sea_orm::{ActiveModelTrait, DatabaseTransaction, DbErr, IntoActiveModel, Set};
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
use super::task_state::change_state;
use super::time::now_millis;
use super::{
    AgentFile, CallerError, CheckpointContent, TaskFilterError, TaskFilters, TaskKeyword,
    TaskResult, TaskState, TaskTitle,
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
    #[error("The task has recorded work; finish it instead of releasing it")]
    HasWork,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error(transparent)]
    InvalidPageSize(#[from] PageSizeError),
    #[error(
        "A one-line reason of 1 to 2000 characters is required for waiting, failed and canceled; other states take no reason."
    )]
    InvalidReason,
    #[error(transparent)]
    NoChanges(#[from] TaskChangeError),
    #[error("Task not found")]
    NotFound,
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
    pub state: TaskState,
    pub person: String,
    pub agent_name: Option<String>,
    pub holder: Option<String>,
    pub state_reason: Option<String>,
    pub planner: Option<String>,
    pub planner_agent_name: Option<String>,
    pub result: String,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub last_activity_at: i64,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct TaskChanges {
    pub title: Option<TaskTitle>,
    pub state: Option<TaskState>,
    #[builder(default = None)]
    pub state_reason: Option<String>,
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
#[error("Provide a title, state or result.")]
pub struct TaskChangeError;

impl TaskChangeError {
    pub const FIELD: &str = "body";
    pub const FIELD_CODE: &str = "required";
}

impl TaskChanges {
    pub fn validate(&self) -> Result<(), TaskChangeError> {
        if self.title.is_none() && self.state.is_none() && self.result.is_none() {
            return Err(TaskChangeError);
        }
        Ok(())
    }
}

impl TryFrom<(task::Model, String)> for Task {
    type Error = DbErr;

    fn try_from((task, person): (task::Model, String)) -> Result<Self, Self::Error> {
        Ok(Self::builder()
            .key(format_key("TK", task.number))
            .task_id(Secret::from(task.random_id))
            .title(task.title)
            .state(
                TaskState::try_from(task.state.as_str())
                    .map_err(|_| DbErr::Type("Invalid task state".into()))?,
            )
            .person(person)
            .agent_name(task.agent_name)
            .holder(task.holder)
            .state_reason(task.state_reason)
            .planner(task.planner_person_id)
            .planner_agent_name(task.planner_agent_name)
            .created_at(task.created_at)
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
            Self::InvalidReason => "task.reason_invalid",
            Self::HasWork => "task.has_work",
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
    pub state: TaskState,
    pub person: String,
    pub agent_name: Option<String>,
    pub holder: Option<String>,
    pub state_reason: Option<String>,
    pub planner: Option<String>,
    pub planner_agent_name: Option<String>,
    pub last_activity_at: i64,
}

impl From<Task> for TaskSummary {
    fn from(task: Task) -> Self {
        Self {
            key: task.key,
            task_id: task.task_id,
            title: task.title,
            state: task.state,
            person: task.person,
            agent_name: task.agent_name,
            holder: task.holder,
            state_reason: task.state_reason,
            planner: task.planner,
            planner_agent_name: task.planner_agent_name,
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
    create_new_task(write, caller, space, title, TaskCreation::Started).await
}

pub async fn plan_task(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    title: TaskTitle,
) -> Result<CreatedTask, TaskError> {
    create_new_task(write, caller, space, title, TaskCreation::Planned).await
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
    task.work_recorded = Set(1);
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
        && changes.state == Some(TaskState::Canceled)
    {
        TaskAction::Cancel
    } else {
        TaskAction::Change
    };
    let handle = write.handle();
    let task = check_task(write.state(), handle, caller, target.check(action)).await?;
    let model = changed_task(task, changes, caller)?;
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
            let roles = if filters.claimable {
                WRITE_ROLES
            } else {
                READ_ROLES
            };
            let authorized = check_space(state, handle, caller, space, roles).await?;
            let version = filters.cursor_version(space).map_err(anyhow::Error::from)?;
            let after = task_position(&options, &version, caller.person_id())?;
            let person_id = match filters.person.as_deref() {
                Some(name) => match state.names().person_id(handle, name).await? {
                    Some(id) => Some(id),
                    None => {
                        return Ok(Page::builder().items(Vec::new()).next_cursor(None).build());
                    }
                },
                None => filters.own_open.then(|| caller.person_id.clone()),
            };
            let filter = listing_filter(&filters, person_id.as_deref());
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

#[derive(Clone, Copy)]
enum TaskCreation {
    Planned,
    Started,
}

async fn create_new_task(
    write: &mut Write<'_>,
    caller: &Caller,
    space: &str,
    title: TaskTitle,
    creation: TaskCreation,
) -> Result<CreatedTask, TaskError> {
    write.begin_operation(Some(caller))?;
    let planned = matches!(creation, TaskCreation::Planned);
    let agent = if planned {
        None
    } else {
        Some(caller.agent()?.to_owned())
    };
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
            agent_name: Set(agent.clone()),
            holder: Set(agent),
            planner_person_id: Set(planned.then(|| caller.person_id.clone())),
            planner_agent_name: Set(planned.then(|| caller.agent_name.clone()).flatten()),
            state_reason: Set(None),
            waiting_request_id: Set(None),
            work_recorded: Set(0),
            created_at: Set(now),
            title: Set(title.into()),
            state: Set(if planned { "planned" } else { "working" }.to_owned()),
            result: Set(String::new()),
            started_at: Set((!planned).then_some(now)),
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

fn changed_task(
    task: task::Model,
    changes: TaskChanges,
    caller: &Caller,
) -> Result<task::ActiveModel, TaskError> {
    let mut task = task;
    if task.state == "planned"
        && (changes
            .state
            .is_some_and(|state| state != TaskState::Canceled)
            || changes.result.is_some())
    {
        return Err(TaskAccessError::StateInvalid(task.state).into());
    }
    if let Some(state) = changes.state {
        change_state(&mut task, state, changes.state_reason, caller)?;
    } else if changes.state_reason.is_some() {
        return Err(TaskError::InvalidReason);
    }
    if let Some(title) = changes.title {
        task.title = title.into();
    }
    if let Some(result) = changes.result {
        task.result = result.into();
        task.work_recorded = 1;
        task.last_activity_at = now_millis()?.max(task.last_activity_at);
    }
    Ok(task.into_active_model().reset_all())
}

async fn names_for(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    tasks: &[task::Model],
) -> anyhow::Result<HashMap<String, String>> {
    let mut ids: Vec<String> = tasks
        .iter()
        .flat_map(|task| [Some(task.person_id.clone()), task.planner_person_id.clone()])
        .flatten()
        .collect();
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
    let planner = task
        .planner_person_id
        .as_ref()
        .map(|id| {
            names
                .get(id)
                .cloned()
                .ok_or_else(|| DbErr::Type("Missing planner person name".into()))
        })
        .transpose()?;
    let mut result = Task::try_from((task, name))?;
    result.planner = planner;
    Ok(result)
}

pub(super) async fn full_task(
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

fn listing_filter<'a>(filters: &'a TaskFilters, person_id: Option<&'a str>) -> TaskListFilter<'a> {
    TaskListFilter {
        person_id,
        agent: filters.agent.as_deref(),
        state: filters
            .claimable
            .then_some(TaskState::Planned)
            .or(filters.state)
            .map(TaskState::as_str),
        own_open: filters.own_open,
        keyword: filters.keyword.as_ref().map(TaskKeyword::as_str),
    }
}
