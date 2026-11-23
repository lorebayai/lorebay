use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use lorebay_lib_core::SpaceRole;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError, ListQuery, Meta, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::domain::{
    SpaceMember, SpaceMemberError, add_space_member, change_space_member_role, list_space_members,
    remove_space_member,
};
use crate::server::ServerState;

use super::person_sign_in::invalid_json;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct RoleRequest {
    role: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct SpaceMemberResponse {
    #[serde(rename = "type")]
    kind: &'static str,
    username: String,
    role: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_name: Option<String>,
}

impl RoleRequest {
    fn added_role(self) -> Result<SpaceRole, ApiError> {
        let role = SpaceRole::try_from(self)?;
        match role {
            SpaceRole::Contributor | SpaceRole::Viewer => Ok(role),
            _ => Err(invalid_role()),
        }
    }
}

impl TryFrom<RoleRequest> for SpaceRole {
    type Error = ApiError;

    fn try_from(request: RoleRequest) -> Result<Self, Self::Error> {
        Self::try_from(request.role.as_str()).map_err(|_| invalid_role())
    }
}

impl From<SpaceMember> for SpaceMemberResponse {
    fn from(space_member: SpaceMember) -> Self {
        Self::builder()
            .kind(if space_member.agent_name.is_some() {
                "agent"
            } else {
                "human"
            })
            .username(space_member.username)
            .role(space_member.role.as_str())
            .agent_name(space_member.agent_name)
            .build()
    }
}

impl From<SpaceMemberError> for ApiError {
    fn from(error: SpaceMemberError) -> Self {
        match error {
            SpaceMemberError::Access(error) => error.into(),
            SpaceMemberError::AlreadyExists => Self::SpaceMemberAlreadyExists,
            SpaceMemberError::Caller(error) => error.into(),
            SpaceMemberError::InvalidCursor => invalid_query("cursor"),
            SpaceMemberError::NotFound => Self::SpaceMemberNotFound,
            SpaceMemberError::UserNotFound => Self::UserNotFound,
            error @ (SpaceMemberError::Database(_) | SpaceMemberError::Internal(_)) => {
                tracing::error!(%error, "space member operation failed");
                Self::Internal
            }
        }
    }
}

pub(crate) async fn list(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    key: Result<Path<String>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Envelope<Vec<SpaceMemberResponse>>, ApiError> {
    let Path(key) = key.map_err(invalid_path)?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let page = list_space_members(&state.database, &caller, &key, query.try_into()?).await?;
    Ok(Envelope::builder()
        .data(
            page.items
                .into_iter()
                .map(SpaceMemberResponse::from)
                .collect::<Vec<_>>(),
        )
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build())
}

pub(crate) async fn add(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, String)>, PathRejection>,
    request: Result<Json<RoleRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<SpaceMemberResponse>), ApiError> {
    let Path((key, username)) = path.map_err(invalid_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let role = request.added_role()?;
    let space_member = add_space_member(&state.database, &caller, &key, &username, role).await?;
    Ok((
        StatusCode::CREATED,
        Envelope::builder().data(space_member).build(),
    ))
}

pub(crate) async fn change_role(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, String)>, PathRejection>,
    request: Result<Json<RoleRequest>, JsonRejection>,
) -> Result<Envelope<SpaceMemberResponse>, ApiError> {
    let Path((key, username)) = path.map_err(invalid_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let space_member = change_space_member_role(
        &state.database,
        &caller,
        &key,
        &username,
        request.try_into()?,
    )
    .await?;
    Ok(Envelope::builder().data(space_member).build())
}

pub(crate) async fn remove(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<(String, String)>, PathRejection>,
) -> Result<Envelope<()>, ApiError> {
    let Path((key, username)) = path.map_err(invalid_path)?;
    remove_space_member(&state.database, &caller, &key, &username).await?;
    Ok(Envelope::builder().data(()).build())
}

fn invalid_role() -> ApiError {
    ApiError::InvalidBody(vec![
        FieldError::builder()
            .field("role")
            .code("invalid")
            .message("Expected an allowed space role.")
            .build(),
    ])
}

fn invalid_path(_error: PathRejection) -> ApiError {
    ApiError::InvalidPath(vec![
        FieldError::builder()
            .field("path")
            .code("invalid")
            .message("Expected a valid UTF-8 space key and username.")
            .build(),
    ])
}
