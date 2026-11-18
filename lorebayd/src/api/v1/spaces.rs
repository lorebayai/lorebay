use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Deserializer};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError, ListQuery, Meta, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::domain::create_space;
use crate::server::ServerState;
use lorebay_lib_core::{NewSpace, Space, SpaceChanges, change_space, list_spaces, read_space};

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

impl TryFrom<CreateSpaceRequest> for NewSpace {
    type Error = ApiError;

    fn try_from(request: CreateSpaceRequest) -> Result<Self, Self::Error> {
        let input = Self::builder()
            .name(request.name)
            .summary(request.summary)
            .build();
        input.validate()?;
        Ok(input)
    }
}

impl TryFrom<ChangeSpaceRequest> for SpaceChanges {
    type Error = ApiError;

    fn try_from(request: ChangeSpaceRequest) -> Result<Self, Self::Error> {
        let input = Self::builder()
            .name(request.name)
            .summary(request.summary)
            .build();
        input.validate()?;
        Ok(input)
    }
}

pub(crate) async fn create(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    request: Result<Json<CreateSpaceRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<Space>), ApiError> {
    let Json(request) = request.map_err(invalid_json)?;
    let space = create_space(&state.library, &caller, request.try_into()?).await?;
    Ok((StatusCode::CREATED, Envelope::builder().data(space).build()))
}

pub(crate) async fn list(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Envelope<Vec<Space>>, ApiError> {
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let page = list_spaces(&state.library, &caller.library_caller(), query.try_into()?).await?;
    Ok(Envelope::builder()
        .data(page.items)
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build())
}

pub(crate) async fn read(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    key: Result<Path<String>, PathRejection>,
) -> Result<Envelope<Space>, ApiError> {
    let Path(key) = key.map_err(invalid_space_path)?;
    let space = read_space(&state.library, &caller.library_caller(), &key).await?;
    Ok(Envelope::builder().data(space).build())
}

pub(crate) async fn change(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    key: Result<Path<String>, PathRejection>,
    request: Result<Json<ChangeSpaceRequest>, JsonRejection>,
) -> Result<Envelope<Space>, ApiError> {
    let Path(key) = key.map_err(invalid_space_path)?;
    let Json(request) = request.map_err(invalid_json)?;
    let caller = caller.library_caller();
    let changes = request.try_into()?;
    let space = state
        .library
        .run_write(Some(&caller), async |write| {
            change_space(write, &caller, &key, changes).await
        })
        .await?;
    Ok(Envelope::builder().data(space).build())
}

fn present_string<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
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
