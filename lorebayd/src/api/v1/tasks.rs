use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use lorebay_base::Secret;
use serde::{Deserialize, Deserializer, Serialize};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError, ListQuery, Meta, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::domain::pagination::{Page, PageOptions};
use crate::domain::{
    CreatedTask, Note, NoteAdded, NoteContent, Task, TaskChanges, TaskError, TaskFilters,
    TaskKeyword, TaskRead, TaskResult, TaskStatus, TaskTextError, TaskTitle, add_note, change_task,
    create_task, delete_note, delete_task, list_tasks, read_scratchpad, read_task,
};
use crate::server::ServerState;

use super::artifacts::ArtifactResponse;
use super::login_session::invalid_json;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateTaskRequest {
    title: String,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct AddNoteRequest {
    content: String,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeTaskRequest {
    #[serde(default, deserialize_with = "present_string")]
    title: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    status: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    result: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct ListTasksQuery {
    page_size: Option<String>,
    cursor: Option<String>,
    person: Option<String>,
    agent: Option<String>,
    status: Option<String>,
    #[serde(default)]
    own_open: bool,
    keyword: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct ReadTaskQuery {
    #[serde(default)]
    include_scratchpad: bool,
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
pub(crate) struct TaskSummaryResponse {
    key: String,
    task_id: Secret<String>,
    title: String,
    status: &'static str,
    person: String,
    agent_name: String,
    last_activity_at: i64,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskResponse {
    #[serde(flatten)]
    summary: TaskSummaryResponse,
    result: String,
    started_at: i64,
    ended_at: Option<i64>,
    artifacts: Vec<ArtifactResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scratchpad: Option<Envelope<Vec<NoteResponse>>>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct NoteResponse {
    time: i64,
    content: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct NoteAddedResponse {
    time: i64,
    task_title: String,
}

impl TryFrom<CreateTaskRequest> for TaskTitle {
    type Error = ApiError;

    fn try_from(request: CreateTaskRequest) -> Result<Self, Self::Error> {
        Self::try_from(request.title).map_err(|error| invalid_text("title", error))
    }
}

impl TryFrom<AddNoteRequest> for NoteContent {
    type Error = ApiError;

    fn try_from(request: AddNoteRequest) -> Result<Self, Self::Error> {
        Self::try_from(request.content).map_err(|error| invalid_text("content", error))
    }
}

impl TryFrom<ChangeTaskRequest> for TaskChanges {
    type Error = ApiError;

    fn try_from(request: ChangeTaskRequest) -> Result<Self, Self::Error> {
        if request.title.is_none() && request.status.is_none() && request.result.is_none() {
            return Err(invalid_field(
                "body",
                "required",
                "Provide a title, status or result.",
            ));
        }
        let title = request
            .title
            .map(TaskTitle::try_from)
            .transpose()
            .map_err(|error| invalid_text("title", error))?;
        let status = request
            .status
            .as_deref()
            .map(TaskStatus::try_from)
            .transpose()
            .map_err(|_| invalid_field("status", "invalid", "Expected in progress or ended."))?;
        let result = request
            .result
            .map(TaskResult::try_from)
            .transpose()
            .map_err(|error| invalid_text("result", error))?;
        Ok(Self::builder()
            .title(title)
            .status(status)
            .result(result)
            .build())
    }
}

impl TryFrom<ListTasksQuery> for (PageOptions, TaskFilters) {
    type Error = ApiError;

    fn try_from(query: ListTasksQuery) -> Result<Self, Self::Error> {
        let status = query
            .status
            .as_deref()
            .map(TaskStatus::try_from)
            .transpose()
            .map_err(|_| invalid_filter("status", "Expected in progress or ended."))?;
        let keyword = query
            .keyword
            .map(TaskKeyword::try_from)
            .transpose()
            .map_err(|error| ApiError::InvalidQuery(vec![text_error("keyword", error)]))?;
        let filters = TaskFilters::builder()
            .person(query.person)
            .agent(query.agent)
            .status(status)
            .own_open(query.own_open)
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
    pub(crate) fn scratchpad(self) -> Result<Option<PageOptions>, ApiError> {
        if !self.include_scratchpad {
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
        Ok(Some(options.try_into()?))
    }
}

impl From<CreatedTask> for CreatedTaskResponse {
    fn from(task: CreatedTask) -> Self {
        Self::builder().key(task.key).task_id(task.task_id).build()
    }
}

impl From<Task> for TaskSummaryResponse {
    fn from(task: Task) -> Self {
        Self::builder()
            .key(task.key)
            .task_id(task.task_id)
            .title(task.title)
            .status(task.status.as_str())
            .person(task.person)
            .agent_name(task.agent_name)
            .last_activity_at(task.last_activity_at)
            .build()
    }
}

impl From<TaskRead> for TaskResponse {
    fn from(read: TaskRead) -> Self {
        Self::builder()
            .result(read.task.result.clone())
            .started_at(read.task.started_at)
            .ended_at(read.task.ended_at)
            .summary(read.task)
            .artifacts(
                read.artifacts
                    .into_iter()
                    .map(ArtifactResponse::from)
                    .collect::<Vec<_>>(),
            )
            .scratchpad(read.scratchpad.map(note_page))
            .build()
    }
}

impl From<Note> for NoteResponse {
    fn from(note: Note) -> Self {
        Self::builder()
            .time(note.time)
            .content(note.content)
            .build()
    }
}

impl From<NoteAdded> for NoteAddedResponse {
    fn from(note: NoteAdded) -> Self {
        Self::builder()
            .time(note.time)
            .task_title(note.task_title)
            .build()
    }
}

impl From<TaskError> for ApiError {
    fn from(error: TaskError) -> Self {
        match error {
            TaskError::Access(error) => error.into(),
            TaskError::Caller(error) => error.into(),
            TaskError::Database(_) | TaskError::Internal(_) => {
                tracing::error!(%error, "task operation failed");
                Self::Internal
            }
            TaskError::Filter(error) => invalid_filter(error.field(), &error.to_string()),
            TaskError::InvalidCursor => invalid_query("cursor"),
            TaskError::NotFound => Self::TaskNotFound,
            TaskError::NoteNotFound => Self::NoteNotFound,
            TaskError::Space(error) => error.into(),
        }
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
    let task = create_task(&state.database, &caller, &space, request.try_into()?).await?;
    Ok((StatusCode::CREATED, Envelope::builder().data(task).build()))
}

pub(crate) async fn append_note(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>)>, PathRejection>,
    request: Result<Json<AddNoteRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<NoteAddedResponse>), ApiError> {
    let Path((space, task_id)) = path.map_err(invalid_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let note = add_note(
        &state.database,
        &caller,
        Some(&space),
        task_id.expose(),
        request.try_into()?,
    )
    .await?;
    Ok((StatusCode::CREATED, Envelope::builder().data(note).build()))
}

pub(crate) async fn change(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>)>, PathRejection>,
    request: Result<Json<ChangeTaskRequest>, JsonRejection>,
) -> Result<Envelope<TaskResponse>, ApiError> {
    let Path((space, task_id)) = path.map_err(invalid_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let task = change_task(
        &state.database,
        &caller,
        Some(&space),
        task_id.expose(),
        request.try_into()?,
    )
    .await?;
    Ok(Envelope::builder().data(task).build())
}

pub(crate) async fn delete(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>)>, PathRejection>,
) -> Result<Envelope<()>, ApiError> {
    let Path((space, task_id)) = path.map_err(invalid_path)?;
    delete_task(
        &state.database,
        state.storage.as_ref(),
        &caller,
        &space,
        task_id.expose(),
    )
    .await?;
    Ok(Envelope::builder().data(()).build())
}

pub(crate) async fn remove_note(
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
                .message("Expected an integer note time.")
                .build(),
        ])
    })?;
    delete_note(&state.database, &caller, &space, task_id.expose(), time).await?;
    Ok(Envelope::builder().data(()).build())
}

pub(crate) async fn list(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<ListTasksQuery>, QueryRejection>,
) -> Result<Envelope<Vec<TaskSummaryResponse>>, ApiError> {
    let Path(space) = path.map_err(invalid_path)?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let (options, filters) = query.try_into()?;
    let page = list_tasks(&state.database, &caller, &space, options, filters).await?;
    Ok(Envelope::builder()
        .data(
            page.items
                .into_iter()
                .map(TaskSummaryResponse::from)
                .collect::<Vec<_>>(),
        )
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
    let task = read_task(&state.database, &caller, &space, &key, query.scratchpad()?).await?;
    Ok(Envelope::builder().data(task).build())
}

pub(crate) async fn scratchpad(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, String)>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Envelope<Vec<NoteResponse>>, ApiError> {
    let Path((space, key)) = path.map_err(invalid_path)?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let page = read_scratchpad(&state.database, &caller, &space, &key, query.try_into()?).await?;
    Ok(note_page(page))
}

pub(crate) fn invalid_text(field: &str, error: TaskTextError) -> ApiError {
    ApiError::InvalidBody(vec![text_error(field, error)])
}

fn note_page(page: Page<Note>) -> Envelope<Vec<NoteResponse>> {
    Envelope::builder()
        .data(
            page.items
                .into_iter()
                .map(NoteResponse::from)
                .collect::<Vec<_>>(),
        )
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build()
}

fn text_error(field: &str, error: TaskTextError) -> FieldError {
    let code = match error {
        TaskTextError::Empty => "required",
        TaskTextError::TooLong(_) => "too_long",
    };
    FieldError::builder()
        .field(field)
        .code(code)
        .message(error.to_string())
        .build()
}

fn invalid_filter(field: &str, message: &str) -> ApiError {
    ApiError::InvalidQuery(vec![
        FieldError::builder()
            .field(field)
            .code("invalid")
            .message(message)
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
