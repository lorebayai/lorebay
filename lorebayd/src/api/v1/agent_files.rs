use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use lorebay_lib_types::Secret;
use serde::Deserialize;
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError};
use crate::domain::caller::VerifiedCaller;
use crate::server::ServerState;
use lorebay_lib_core::{
    AgentFileCompleted, AgentFileName, AgentFileRegistered, AgentFileSummary, AgentFileTextError,
    DownloadRequest, NewAgentFile, TaskTarget, complete_agent_file, delete_agent_file,
    get_agent_file_download_url, prepare_agent_file_completion, register_agent_file,
};

use super::login_session::invalid_json;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct RegisterAgentFileRequest {
    file_name: String,
    summary: String,
}

impl TryFrom<RegisterAgentFileRequest> for NewAgentFile {
    type Error = ApiError;

    fn try_from(request: RegisterAgentFileRequest) -> Result<Self, Self::Error> {
        Ok(Self::builder()
            .file_name(
                AgentFileName::try_from(request.file_name)
                    .map_err(|error| invalid_text("file_name", error))?,
            )
            .summary(
                AgentFileSummary::try_from(request.summary)
                    .map_err(|error| invalid_text("summary", error))?,
            )
            .build())
    }
}

pub(crate) async fn download_url(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Result<Envelope<DownloadRequest>, ApiError> {
    let Path((space, key)) = path.map_err(invalid_path)?;
    let download =
        get_agent_file_download_url(&state.library, &caller.library_caller(), &space, &key).await?;
    Ok(Envelope::builder().data(download).build())
}

pub(crate) async fn register(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>)>, PathRejection>,
    request: Result<Json<RegisterAgentFileRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<AgentFileRegistered>), ApiError> {
    let Path((space, task_id)) = path.map_err(invalid_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let target = TaskTarget::builder()
        .path_space(Some(space.as_str()))
        .task_id(task_id.expose().as_str())
        .build();
    let caller = caller.library_caller();
    let input = request.try_into()?;
    let registration = state
        .library
        .run_write(async |write| register_agent_file(write, &caller, target, input).await)
        .await?;
    let agent_file = registration.issue_upload(&state.library).await?;
    Ok((
        StatusCode::CREATED,
        Envelope::builder().data(agent_file).build(),
    ))
}

pub(crate) async fn complete(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>, String)>, PathRejection>,
) -> Result<Envelope<AgentFileCompleted>, ApiError> {
    let Path((space, task_id, key)) = path.map_err(invalid_path)?;
    let target = TaskTarget::builder()
        .path_space(Some(space.as_str()))
        .task_id(task_id.expose().as_str())
        .build();
    let caller = caller.library_caller();
    let prepared = prepare_agent_file_completion(&state.library, &caller, target, &key).await?;
    let completion = state
        .library
        .run_write(async |write| complete_agent_file(write, &caller, target, prepared).await)
        .await?;
    let agent_file = completion.into_result()?;
    Ok(Envelope::builder().data(agent_file).build())
}

pub(crate) async fn delete(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>, String)>, PathRejection>,
) -> Result<Envelope<()>, ApiError> {
    let Path((space, task_id, key)) = path.map_err(invalid_path)?;
    let target = TaskTarget::builder()
        .path_space(Some(space.as_str()))
        .task_id(task_id.expose().as_str())
        .build();
    let caller = caller.library_caller();
    state
        .library
        .run_write(async |write| delete_agent_file(write, &caller, target, &key).await)
        .await?;
    Ok(Envelope::builder().data(()).build())
}

fn invalid_text(field: &str, error: AgentFileTextError) -> ApiError {
    let code = match error {
        AgentFileTextError::Empty => "required",
        AgentFileTextError::TooManyBytes(_) | AgentFileTextError::TooManyCharacters(_) => {
            "too_long"
        }
        _ => "invalid",
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
            .message("Expected valid UTF-8 space, task and file identifiers.")
            .build(),
    ])
}
