use std::collections::BTreeMap;

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use lorebay_base::Secret;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError};
use crate::domain::caller::VerifiedCaller;
use crate::domain::{
    Artifact, ArtifactCompleted, ArtifactError, ArtifactFileName, ArtifactRegistered,
    ArtifactSummary, ArtifactTarget, ArtifactTextError, NewArtifact, complete_artifact,
    get_artifact_download_url, register_artifact,
};
use crate::server::ServerState;
use crate::storage::DownloadRequest;

use super::login_session::invalid_json;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct RegisterArtifactRequest {
    file_name: String,
    summary: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ArtifactResponse {
    key: String,
    file_name: String,
    summary: String,
    size: i64,
    created_at: i64,
    completed_at: i64,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ArtifactCompletedResponse {
    #[serde(flatten)]
    artifact: ArtifactResponse,
    task_title: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ArtifactRegisteredResponse {
    key: String,
    upload_url: Secret<String>,
    method: String,
    headers: BTreeMap<String, String>,
    expires_at: i64,
    task_title: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ArtifactDownloadResponse {
    url: Secret<String>,
    expires_at: i64,
}

impl From<DownloadRequest> for ArtifactDownloadResponse {
    fn from(request: DownloadRequest) -> Self {
        Self::builder()
            .url(request.url)
            .expires_at(request.expires_at)
            .build()
    }
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

impl From<Artifact> for ArtifactResponse {
    fn from(artifact: Artifact) -> Self {
        Self::builder()
            .key(artifact.key)
            .file_name(artifact.file_name)
            .summary(artifact.summary)
            .size(artifact.size)
            .created_at(artifact.created_at)
            .completed_at(artifact.completed_at)
            .build()
    }
}

impl From<ArtifactCompleted> for ArtifactCompletedResponse {
    fn from(completed: ArtifactCompleted) -> Self {
        Self::builder()
            .artifact(completed.artifact)
            .task_title(completed.task_title)
            .build()
    }
}

impl From<ArtifactRegistered> for ArtifactRegisteredResponse {
    fn from(registered: ArtifactRegistered) -> Self {
        Self::builder()
            .key(registered.key)
            .upload_url(registered.upload.url)
            .method(registered.upload.method)
            .headers(registered.upload.headers)
            .expires_at(registered.upload.expires_at)
            .task_title(registered.task_title)
            .build()
    }
}

impl From<ArtifactError> for ApiError {
    fn from(error: ArtifactError) -> Self {
        match error {
            ArtifactError::Access(error) => error.into(),
            ArtifactError::Caller(error) => error.into(),
            ArtifactError::Database(_) | ArtifactError::Internal(_) => {
                tracing::error!(%error, "artifact operation failed");
                Self::Internal
            }
            ArtifactError::NotFound => Self::ArtifactNotFound,
            ArtifactError::NotUploaded => Self::ArtifactNotUploaded,
            ArtifactError::Space(error) => error.into(),
        }
    }
}

pub(crate) async fn download_url(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Result<Envelope<ArtifactDownloadResponse>, ApiError> {
    let Path((space, key)) = path.map_err(invalid_path)?;
    let download = get_artifact_download_url(
        &state.database,
        state.storage.as_ref(),
        &caller,
        &space,
        &key,
    )
    .await?;
    Ok(Envelope::builder().data(download).build())
}

pub(crate) async fn register(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, Secret<String>)>, PathRejection>,
    request: Result<Json<RegisterArtifactRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<ArtifactRegisteredResponse>), ApiError> {
    let Path((space, task_id)) = path.map_err(invalid_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let target = ArtifactTarget::builder()
        .path_space(Some(space.as_str()))
        .task_id(&task_id)
        .build();
    let artifact = register_artifact(
        &state.database,
        state.storage.as_ref(),
        &caller,
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
) -> Result<Envelope<ArtifactCompletedResponse>, ApiError> {
    let Path((space, task_id, key)) = path.map_err(invalid_path)?;
    let target = ArtifactTarget::builder()
        .path_space(Some(space.as_str()))
        .task_id(&task_id)
        .build();
    let artifact = complete_artifact(
        &state.database,
        state.storage.as_ref(),
        &caller,
        target,
        &key,
    )
    .await?;
    Ok(Envelope::builder().data(artifact).build())
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
