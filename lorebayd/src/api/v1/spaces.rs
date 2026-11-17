use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Deserializer, Serialize};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError, ListQuery, Meta, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::domain::{
    NewSpace, Space, SpaceChanges, SpaceError, change_space, create_space, list_spaces, read_space,
};
use crate::server::ServerState;

use super::login_session::invalid_json;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateSpaceRequest {
    name: String,
    summary: String,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeSpaceRequest {
    #[serde(default, deserialize_with = "present_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct SpaceResponse {
    key: String,
    name: String,
    summary: String,
    role: &'static str,
}

impl TryFrom<CreateSpaceRequest> for NewSpace {
    type Error = ApiError;

    fn try_from(request: CreateSpaceRequest) -> Result<Self, Self::Error> {
        validate_field("name", &request.name, NAME_MAX_BYTES)?;
        validate_field("summary", &request.summary, SUMMARY_MAX_BYTES)?;
        Ok(Self::builder()
            .name(request.name)
            .summary(request.summary)
            .build())
    }
}

impl TryFrom<ChangeSpaceRequest> for SpaceChanges {
    type Error = ApiError;

    fn try_from(request: ChangeSpaceRequest) -> Result<Self, Self::Error> {
        if request.name.is_none() && request.summary.is_none() {
            return Err(invalid_field(
                "body",
                "required",
                "Provide a name or summary.",
            ));
        }
        if let Some(name) = &request.name {
            validate_field("name", name, NAME_MAX_BYTES)?;
        }
        if let Some(summary) = &request.summary {
            validate_field("summary", summary, SUMMARY_MAX_BYTES)?;
        }
        Ok(Self::builder()
            .name(request.name)
            .summary(request.summary)
            .build())
    }
}

impl From<Space> for SpaceResponse {
    fn from(space: Space) -> Self {
        Self::builder()
            .key(space.key)
            .name(space.name)
            .summary(space.summary)
            .role(space.role.as_str())
            .build()
    }
}

impl From<SpaceError> for ApiError {
    fn from(error: SpaceError) -> Self {
        match error {
            SpaceError::Access(error) => error.into(),
            SpaceError::Caller(error) => error.into(),
            SpaceError::InvalidCursor => invalid_query("cursor"),
            error @ (SpaceError::Database(_) | SpaceError::Internal(_)) => {
                tracing::error!(%error, "space operation failed");
                Self::Internal
            }
        }
    }
}

pub(crate) async fn create(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    request: Result<Json<CreateSpaceRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<SpaceResponse>), ApiError> {
    let Json(request) = request.map_err(invalid_json)?;
    let space = create_space(&state.database, &caller, request.try_into()?).await?;
    Ok((StatusCode::CREATED, Envelope::builder().data(space).build()))
}

pub(crate) async fn list(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Envelope<Vec<SpaceResponse>>, ApiError> {
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let page = list_spaces(&state.database, &caller, query.try_into()?).await?;
    Ok(Envelope::builder()
        .data(
            page.items
                .into_iter()
                .map(SpaceResponse::from)
                .collect::<Vec<_>>(),
        )
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build())
}

pub(crate) async fn read(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    key: Result<Path<String>, PathRejection>,
) -> Result<Envelope<SpaceResponse>, ApiError> {
    let Path(key) = key.map_err(invalid_space_path)?;
    let space = read_space(&state.database, &caller, &key).await?;
    Ok(Envelope::builder().data(space).build())
}

pub(crate) async fn change(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    key: Result<Path<String>, PathRejection>,
    request: Result<Json<ChangeSpaceRequest>, JsonRejection>,
) -> Result<Envelope<SpaceResponse>, ApiError> {
    let Path(key) = key.map_err(invalid_space_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let space = change_space(&state.database, &caller, &key, request.try_into()?).await?;
    Ok(Envelope::builder().data(space).build())
}

const NAME_MAX_BYTES: usize = 256;
const SUMMARY_MAX_BYTES: usize = 4096;

fn present_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

fn validate_field(field: &str, value: &str, max_bytes: usize) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(invalid_field(field, "required", "Must not be empty."));
    }
    if value.len() > max_bytes {
        return Err(invalid_field(
            field,
            "too_long",
            &format!("Must be at most {max_bytes} UTF-8 bytes."),
        ));
    }
    Ok(())
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

fn invalid_space_path(_error: PathRejection) -> ApiError {
    ApiError::InvalidPath(vec![
        FieldError::builder()
            .field("space")
            .code("invalid")
            .message("Expected a valid UTF-8 space key.")
            .build(),
    ])
}
