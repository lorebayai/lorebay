use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use lorebay_lib_types::Secret;
use serde::{Deserialize, Deserializer, Serialize};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError, ListQuery, Meta, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::server::ServerState;
use lorebay_lib_core::{
    Checkpoint, CheckpointAdded, CheckpointContent, CreatedTask, TaskChanges, TaskError,
    TaskFilters, TaskKeyword, TaskRead, TaskResult, TaskState, TaskTextError, TaskTitle,
    add_checkpoint, change_task, create_task, delete_checkpoint, delete_task, list_tasks,
    plan_task, read_scratchpad, read_task,
};
use lorebay_lib_core::{
    Page, PageOptions, TaskInclude, TaskResultVersion, TaskStateError, TaskSummary, TaskTarget,
    read_task_results,
};

use super::person_sign_in::invalid_json;
use lorebay_lib_core::AgentFile;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateTaskRequest {
    title: String,
    #[serde(default)]
    planned: bool,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct AddCheckpointRequest {
    content: String,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeTaskRequest {
    #[serde(default, deserialize_with = "present_string")]
    title: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    state: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    state_reason: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    result: Option<String>,
    result_version: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct ListTasksQuery {
    page_size: Option<String>,
    cursor: Option<String>,
    person: Option<String>,
    agent: Option<String>,
    state: Option<String>,
    #[serde(default)]
    own_open: bool,
    #[serde(default)]
    claimable: bool,
    keyword: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadTaskQuery {
    #[serde(default)]
    include_scratchpad: bool,
    #[serde(default)]
    include_results: bool,
    page_size: Option<String>,
    cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct CreatedTaskResponse {
    key: String,
    task_id: Secret<String>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskResponse {
    #[serde(flatten)]
    summary: TaskSummary,
    state_changed_by: String,
    state_changed_agent_name: Option<String>,
    state_changed_at: i64,
    result: String,
    result_version: i64,
    created_at: i64,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    files: Vec<AgentFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scratchpad: Option<Envelope<Vec<Checkpoint>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    results: Option<Envelope<Vec<TaskResultVersion>>>,
}

impl TryFrom<CreateTaskRequest> for TaskTitle {
    type Error = ApiError;

    fn try_from(request: CreateTaskRequest) -> Result<Self, Self::Error> {
        Self::try_from(request.title).map_err(|error| invalid_text("title", error))
    }
}

impl TryFrom<AddCheckpointRequest> for CheckpointContent {
    type Error = ApiError;

    fn try_from(request: AddCheckpointRequest) -> Result<Self, Self::Error> {
        Self::try_from(request.content).map_err(|error| invalid_text("content", error))
    }
}

impl TryFrom<ChangeTaskRequest> for TaskChanges {
    type Error = ApiError;

    fn try_from(request: ChangeTaskRequest) -> Result<Self, Self::Error> {
        if request.result_version.is_some_and(|version| version <= 0) {
            return Err(invalid_field(
                "result_version",
                "invalid",
                "Expected a positive result version or null.",
            ));
        }
        let title = request
            .title
            .map(TaskTitle::try_from)
            .transpose()
            .map_err(|error| invalid_text("title", error))?;
        let state = request
            .state
            .as_deref()
            .map(TaskState::try_from)
            .transpose()
            .map_err(|error| {
                invalid_field(
                    TaskStateError::FIELD,
                    TaskStateError::FIELD_CODE,
                    &error.to_string(),
                )
            })?;
        let result = request
            .result
            .map(TaskResult::try_from)
            .transpose()
            .map_err(|error| invalid_text("result", error))?;
        let changes = Self::builder()
            .title(title)
            .state(state)
            .state_reason(request.state_reason)
            .result(result)
            .result_version(request.result_version)
            .build();
        changes.validate()?;
        Ok(changes)
    }
}

impl TryFrom<ListTasksQuery> for (PageOptions, TaskFilters) {
    type Error = ApiError;

    fn try_from(query: ListTasksQuery) -> Result<Self, Self::Error> {
        let state = query
            .state
            .as_deref()
            .map(TaskState::try_from)
            .transpose()
            .map_err(invalid_filter)?;
        let keyword = query
            .keyword
            .map(TaskKeyword::try_from)
            .transpose()
            .map_err(|error| ApiError::InvalidQuery(vec![text_error("keyword", error)]))?;
        let filters = TaskFilters::builder()
            .person(query.person)
            .agent(query.agent)
            .state(state)
            .own_open(query.own_open)
            .claimable(query.claimable)
            .keyword(keyword)
            .build();
        filters.validate().map_err(TaskError::from)?;
        let page = ListQuery::builder()
            .page_size(query.page_size)
            .cursor(query.cursor)
            .build()
            .try_into()?;
        Ok((page, filters))
    }
}

impl ReadTaskQuery {
    pub(crate) fn include(self) -> Result<Option<TaskInclude>, ApiError> {
        if self.include_scratchpad && self.include_results {
            return Err(invalid_query("query"));
        }
        if !self.include_scratchpad && !self.include_results {
            if self.page_size.is_some() {
                return Err(invalid_query("page_size"));
            }
            if self.cursor.is_some() {
                return Err(invalid_query("cursor"));
            }
            return Ok(None);
        }
        let options = ListQuery::builder()
            .page_size(self.page_size)
            .cursor(self.cursor)
            .build();
        let options = options.try_into()?;
        Ok(Some(if self.include_results {
            TaskInclude::Results(options)
        } else {
            TaskInclude::Scratchpad(options)
        }))
    }
}

impl From<CreatedTask> for CreatedTaskResponse {
    fn from(task: CreatedTask) -> Self {
        Self::builder().key(task.key).task_id(task.task_id).build()
    }
}

impl From<TaskRead> for TaskResponse {
    fn from(read: TaskRead) -> Self {
        Self::builder()
            .state_changed_by(read.task.state_changed_by.clone())
            .state_changed_agent_name(read.task.state_changed_agent_name.clone())
            .state_changed_at(read.task.state_changed_at)
            .result(read.task.result.clone())
            .result_version(read.task.result_version)
            .created_at(read.task.created_at)
            .started_at(read.task.started_at)
            .ended_at(read.task.ended_at)
            .summary(TaskSummary::from(read.task))
            .files(read.files)
            .scratchpad(read.scratchpad.map(checkpoint_page))
            .results(read.results.map(result_page))
            .build()
    }
}

pub(crate) async fn create(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<String>, PathRejection>,
    request: Result<Json<CreateTaskRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<CreatedTaskResponse>), ApiError> {
    let Path(space) = path.map_err(invalid_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let caller = caller.library_caller();
    let planned = request.planned;
    let title = request.try_into()?;
    let task = state
        .library
        .run_write(async |write| {
            if planned {
                plan_task(write, &caller, &space, title).await
            } else {
                create_task(write, &caller, &space, title).await
            }
        })
        .await?;
    Ok((StatusCode::CREATED, Envelope::builder().data(task).build()))
}

pub(crate) async fn append_checkpoint(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>)>, PathRejection>,
    request: Result<Json<AddCheckpointRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<CheckpointAdded>), ApiError> {
    let Path((space, task_id)) = path.map_err(invalid_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let caller = caller.library_caller();
    let target = TaskTarget {
        path_space: Some(&space),
        task_id: task_id.expose(),
    };
    let input = request.try_into()?;
    let checkpoint = state
        .library
        .run_write(async |write| add_checkpoint(write, &caller, target, input).await)
        .await?;
    Ok((
        StatusCode::CREATED,
        Envelope::builder().data(checkpoint).build(),
    ))
}

pub(crate) async fn change(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>)>, PathRejection>,
    request: Result<Json<ChangeTaskRequest>, JsonRejection>,
) -> Result<Envelope<TaskResponse>, ApiError> {
    let Path((space, task_id)) = path.map_err(invalid_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let caller = caller.library_caller();
    let target = TaskTarget {
        path_space: Some(&space),
        task_id: task_id.expose(),
    };
    let input = request.try_into()?;
    let task = state
        .library
        .run_write(async |write| change_task(write, &caller, target, input).await)
        .await?;
    Ok(Envelope::builder().data(task).build())
}

pub(crate) async fn delete(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>)>, PathRejection>,
) -> Result<Envelope<()>, ApiError> {
    let Path((space, task_id)) = path.map_err(invalid_path)?;
    let caller = caller.library_caller();
    let target = TaskTarget {
        path_space: Some(&space),
        task_id: task_id.expose(),
    };
    state
        .library
        .run_write(async |write| delete_task(write, &caller, target).await)
        .await?;
    Ok(Envelope::builder().data(()).build())
}

pub(crate) async fn remove_checkpoint(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>, String)>, PathRejection>,
) -> Result<Envelope<()>, ApiError> {
    let Path((space, task_id, time)) = path.map_err(invalid_path)?;
    let time = time.parse::<i64>().map_err(|_| {
        ApiError::InvalidPath(vec![
            FieldError::builder()
                .field("time")
                .code("invalid")
                .message("Expected an integer checkpoint time.")
                .build(),
        ])
    })?;
    let caller = caller.library_caller();
    let target = TaskTarget {
        path_space: Some(&space),
        task_id: task_id.expose(),
    };
    state
        .library
        .run_write(async |write| delete_checkpoint(write, &caller, target, time).await)
        .await?;
    Ok(Envelope::builder().data(()).build())
}

pub(crate) async fn list(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<ListTasksQuery>, QueryRejection>,
) -> Result<Envelope<Vec<TaskSummary>>, ApiError> {
    let Path(space) = path.map_err(invalid_path)?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let (options, filters) = query.try_into()?;
    let page = list_tasks(
        &state.library,
        &caller.library_caller(),
        &space,
        options,
        filters,
    )
    .await?;
    Ok(Envelope::builder()
        .data(page.items)
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build())
}

pub(crate) async fn read(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, String)>, PathRejection>,
    query: Result<Query<ReadTaskQuery>, QueryRejection>,
) -> Result<Envelope<TaskResponse>, ApiError> {
    let Path((space, key)) = path.map_err(invalid_path)?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let task = read_task(
        &state.library,
        &caller.library_caller(),
        &space,
        &key,
        query.include()?,
    )
    .await?;
    Ok(Envelope::builder().data(task).build())
}

pub(crate) async fn scratchpad(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, String)>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Envelope<Vec<Checkpoint>>, ApiError> {
    let Path((space, key)) = path.map_err(invalid_path)?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let page = read_scratchpad(
        &state.library,
        &caller.library_caller(),
        &space,
        &key,
        query.try_into()?,
    )
    .await?;
    Ok(checkpoint_page(page))
}

pub(crate) async fn results(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, String)>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Envelope<Vec<TaskResultVersion>>, ApiError> {
    let Path((space, key)) = path.map_err(invalid_path)?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let page = read_task_results(
        &state.library,
        &caller.library_caller(),
        &space,
        &key,
        query.try_into()?,
    )
    .await?;
    Ok(result_page(page))
}

fn result_page(page: Page<TaskResultVersion>) -> Envelope<Vec<TaskResultVersion>> {
    Envelope::builder()
        .data(page.items)
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build()
}

pub(crate) fn invalid_text(field: &str, error: TaskTextError) -> ApiError {
    ApiError::InvalidBody(vec![text_error(field, error)])
}

fn checkpoint_page(page: Page<Checkpoint>) -> Envelope<Vec<Checkpoint>> {
    Envelope::builder()
        .data(page.items)
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build()
}

fn text_error(field: &str, error: TaskTextError) -> FieldError {
    let code = match error {
        TaskTextError::Empty => "required",
        TaskTextError::TooLong(_) => "too_long",
        _ => "invalid",
    };
    FieldError::builder()
        .field(field)
        .code(code)
        .message(error.to_string())
        .build()
}

fn invalid_filter(error: TaskStateError) -> ApiError {
    ApiError::InvalidQuery(vec![
        FieldError::builder()
            .field(TaskStateError::FIELD)
            .code(TaskStateError::FIELD_CODE)
            .message(error.to_string())
            .build(),
    ])
}

fn invalid_field(field: &str, code: &str, message: &str) -> ApiError {
    ApiError::InvalidBody(vec![
        FieldError::builder()
            .field(field)
            .code(code)
            .message(message)
            .build(),
    ])
}

fn present_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

fn invalid_path(_error: PathRejection) -> ApiError {
    ApiError::InvalidPath(vec![
        FieldError::builder()
            .field("path")
            .code("invalid")
            .message("Expected valid UTF-8 space and task identifiers.")
            .build(),
    ])
}
