use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use lorebay_base::Secret;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError, ListQuery, Meta, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::domain::pagination::{Page, PageOptions};
use crate::domain::{
    CreatedTask, Note, NoteAdded, NoteContent, Task, TaskError, TaskRead, TaskTextError, TaskTitle,
    add_note, create_task, list_tasks, read_scratchpad, read_task,
};
use crate::server::ServerState;

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
    artifacts: Vec<()>,
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

impl ReadTaskQuery {
    fn scratchpad(self) -> Result<Option<PageOptions>, ApiError> {
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
            .artifacts(Vec::new())
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
            TaskError::InvalidCursor => invalid_query("cursor"),
            TaskError::NotFound => Self::TaskNotFound,
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

pub(crate) async fn list(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Envelope<Vec<TaskSummaryResponse>>, ApiError> {
    let Path(space) = path.map_err(invalid_path)?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let page = list_tasks(&state.database, &caller, &space, query.try_into()?).await?;
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

fn invalid_text(field: &str, error: TaskTextError) -> ApiError {
    let code = match error {
        TaskTextError::Empty => "required",
        TaskTextError::TooLong(_) => "too_long",
    };
    ApiError::InvalidBody(vec![
        FieldError::builder()
            .field(field)
            .code(code)
            .message(error.to_string())
            .build(),
    ])
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
