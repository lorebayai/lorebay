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
    ArtifactCompleted, ArtifactFileName, ArtifactRegistered, ArtifactSummary, ArtifactTextError,
    DownloadRequest, NewArtifact, TaskTarget, WriteOperation, complete_artifact, delete_artifact,
    get_artifact_download_url, register_artifact,
};

use super::login_session::invalid_json;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct RegisterArtifactRequest {
    file_name: String,
    summary: String,
}

impl TryFrom<RegisterArtifactRequest> for NewArtifact {
    type Error = ApiError;

    fn try_from(request: RegisterArtifactRequest) -> Result<Self, Self::Error> {
        Ok(Self::builder()
            .file_name(
                ArtifactFileName::try_from(request.file_name)
                    .map_err(|error| invalid_text("file_name", error))?,
            )
            .summary(
                ArtifactSummary::try_from(request.summary)
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
        get_artifact_download_url(&state.library, &caller.library_caller(), &space, &key).await?;
    Ok(Envelope::builder().data(download).build())
}

pub(crate) async fn register(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>)>, PathRejection>,
    request: Result<Json<RegisterArtifactRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<ArtifactRegistered>), ApiError> {
    let Path((space, task_id)) = path.map_err(invalid_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let target = TaskTarget::builder()
        .path_space(Some(space.as_str()))
        .task_id(task_id.expose().as_str())
        .build();
    let artifact = register_artifact(
        &state.library,
        &caller.library_caller(),
        target,
        request.try_into()?,
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Envelope::builder().data(artifact).build(),
    ))
}

pub(crate) async fn complete(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>, String)>, PathRejection>,
) -> Result<Envelope<ArtifactCompleted>, ApiError> {
    let Path((space, task_id, key)) = path.map_err(invalid_path)?;
    let target = TaskTarget::builder()
        .path_space(Some(space.as_str()))
        .task_id(task_id.expose().as_str())
        .build();
    let artifact =
        complete_artifact(&state.library, &caller.library_caller(), target, &key).await?;
    Ok(Envelope::builder().data(artifact).build())
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
        .run_write(
            WriteOperation::DeleteArtifact,
            Some(&caller),
            async |write| delete_artifact(write, &caller, target, &key).await,
        )
        .await?;
    Ok(Envelope::builder().data(()).build())
}

fn invalid_text(field: &str, error: ArtifactTextError) -> ApiError {
    let code = match error {
        ArtifactTextError::Empty => "required",
        ArtifactTextError::TooManyBytes(_) | ArtifactTextError::TooManyCharacters(_) => "too_long",
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
            .message("Expected valid UTF-8 space, task and artifact identifiers.")
            .build(),
    ])
}
