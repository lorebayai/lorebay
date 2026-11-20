use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use lorebay_lib_core::{
    DeletedKnowledgeEntry, KNOWLEDGE_FIELDS_MAX_JSON_BYTES, KNOWLEDGE_NAME_MAX_CHARACTERS,
    KnowledgeChanges, KnowledgeEntry, KnowledgeFilters, KnowledgeInputError, KnowledgeListItem,
    KnowledgeTarget, KnowledgeVersion, NewKnowledgeEntry, PageOptions, change_knowledge_entry,
    create_knowledge_entry, delete_knowledge_entry, list_deleted_knowledge, list_knowledge,
    list_knowledge_versions, read_knowledge_entry, read_knowledge_version,
    roll_back_knowledge_entry,
};
use serde::{Deserialize, Deserializer};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, Meta, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::server::ServerState;

use super::login_session::invalid_json;

// Proposals also name a target entry; the envelope allowance covers field names and metadata.
pub(crate) const KNOWLEDGE_BODY_MAX_BYTES: usize =
    KNOWLEDGE_FIELDS_MAX_JSON_BYTES + KNOWLEDGE_NAME_MAX_CHARACTERS * 12 + 1024;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateRequest {
    name: String,
    summary: String,
    content: String,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangeRequest {
    #[serde(default, deserialize_with = "present_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    summary: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    content: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct EntryPath {
    space: String,
    key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct TargetQuery {
    name: Option<String>,
    key: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeQuery {
    prefix: Option<String>,
    keyword: Option<String>,
    page_size: Option<String>,
    cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct VersionsQuery {
    name: Option<String>,
    key: Option<String>,
    page_size: Option<String>,
    cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct VersionPath {
    space: String,
    key: Option<String>,
    version: i64,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct RollbackRequest {
    version: i64,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct DeletedQuery {
    page_size: Option<String>,
    cursor: Option<String>,
}

impl EntryPath {
    fn target(&self, query: TargetQuery) -> Result<KnowledgeTarget, ApiError> {
        if self.key.is_some() && query.key.is_some() {
            return Err(KnowledgeInputError::InvalidTarget.into());
        }
        KnowledgeTarget::new(query.name, self.key.clone().or(query.key)).map_err(Into::into)
    }
}

impl TryFrom<CreateRequest> for NewKnowledgeEntry {
    type Error = ApiError;

    fn try_from(request: CreateRequest) -> Result<Self, Self::Error> {
        Self::new(request.name, request.summary, request.content).map_err(Into::into)
    }
}

impl TryFrom<ChangeRequest> for KnowledgeChanges {
    type Error = ApiError;

    fn try_from(request: ChangeRequest) -> Result<Self, Self::Error> {
        Self::new(request.name, request.summary, request.content).map_err(Into::into)
    }
}

pub(crate) async fn create(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<String>, PathRejection>,
    request: Result<Json<CreateRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<KnowledgeEntry>), ApiError> {
    let Path(space) = path.map_err(|_| invalid_query("space"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let input = request.try_into()?;
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| create_knowledge_entry(write, &caller, &space, input).await)
        .await?;
    Ok((
        StatusCode::CREATED,
        Envelope::builder().data(result).build(),
    ))
}

pub(crate) async fn change(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<EntryPath>, PathRejection>,
    query: Result<Query<TargetQuery>, QueryRejection>,
    request: Result<Json<ChangeRequest>, JsonRejection>,
) -> Result<Envelope<KnowledgeEntry>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let target = path.target(query)?;
    let changes = request.try_into()?;
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| {
            change_knowledge_entry(write, &caller, &path.space, target, changes).await
        })
        .await?;
    Ok(Envelope::builder().data(result).build())
}

pub(crate) async fn delete(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<EntryPath>, PathRejection>,
    query: Result<Query<TargetQuery>, QueryRejection>,
) -> Result<Envelope<KnowledgeEntry>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let target = path.target(query)?;
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| delete_knowledge_entry(write, &caller, &path.space, target).await)
        .await?;
    Ok(Envelope::builder().data(result).build())
}

pub(crate) async fn read(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<EntryPath>, PathRejection>,
    query: Result<Query<TargetQuery>, QueryRejection>,
) -> Result<Envelope<KnowledgeEntry>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let result = read_knowledge_entry(
        &state.library,
        &caller.library_caller(),
        &path.space,
        path.target(query)?,
    )
    .await?;
    Ok(Envelope::builder().data(result).build())
}

pub(crate) async fn list(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<KnowledgeQuery>, QueryRejection>,
) -> Result<Envelope<Vec<KnowledgeListItem>>, ApiError> {
    let Path(space) = path.map_err(|_| invalid_query("space"))?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let options = PageOptions::from_text(query.page_size.as_deref(), query.cursor)
        .map_err(|_| invalid_query("page_size"))?;
    let filters = KnowledgeFilters::new(query.prefix, query.keyword)?;
    let page = list_knowledge(
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

pub(crate) async fn versions(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<EntryPath>, PathRejection>,
    query: Result<Query<VersionsQuery>, QueryRejection>,
) -> Result<Envelope<Vec<KnowledgeVersion>>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let target = path.target(TargetQuery {
        name: query.name,
        key: query.key,
    })?;
    let options = PageOptions::from_text(query.page_size.as_deref(), query.cursor)
        .map_err(|_| invalid_query("page_size"))?;
    let page = list_knowledge_versions(
        &state.library,
        &caller.library_caller(),
        &path.space,
        target,
        options,
    )
    .await?;
    Ok(Envelope::builder()
        .data(page.items)
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build())
}

pub(crate) async fn read_version(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<VersionPath>, PathRejection>,
    query: Result<Query<TargetQuery>, QueryRejection>,
) -> Result<Envelope<KnowledgeEntry>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let entry = EntryPath {
        space: path.space,
        key: path.key,
    };
    let result = read_knowledge_version(
        &state.library,
        &caller.library_caller(),
        &entry.space,
        entry.target(query)?,
        path.version,
    )
    .await?;
    Ok(Envelope::builder().data(result).build())
}

pub(crate) async fn deleted(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<DeletedQuery>, QueryRejection>,
) -> Result<Envelope<Vec<DeletedKnowledgeEntry>>, ApiError> {
    let Path(space) = path.map_err(|_| invalid_query("space"))?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let options = PageOptions::from_text(query.page_size.as_deref(), query.cursor)
        .map_err(|_| invalid_query("page_size"))?;
    let page =
        list_deleted_knowledge(&state.library, &caller.library_caller(), &space, options).await?;
    Ok(Envelope::builder()
        .data(page.items)
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build())
}

pub(crate) async fn roll_back(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<EntryPath>, PathRejection>,
    query: Result<Query<TargetQuery>, QueryRejection>,
    request: Result<Json<RollbackRequest>, JsonRejection>,
) -> Result<Envelope<KnowledgeEntry>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let target = path.target(query)?;
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| {
            roll_back_knowledge_entry(write, &caller, &path.space, target, request.version).await
        })
        .await?;
    Ok(Envelope::builder().data(result).build())
}

pub(super) fn present_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}
