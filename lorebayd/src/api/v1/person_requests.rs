use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use lorebay_lib_core::{
    KnowledgeChanges, KnowledgeTarget, NewKnowledgeEntry, PageOptions, PersonRequest,
    PersonRequestContent, PersonRequestFilters, PersonRequestListItem, PersonRequestRejection,
    PersonRequestSource, PersonRequestStatus, RevisedPersonRequest, SubmittedPersonRequest,
    TaskTarget, approve_person_request, list_person_requests, read_person_request,
    reject_person_request, revise_person_request, submit_person_request,
};
use lorebay_lib_types::Secret;
use serde::Deserialize;
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError, Meta, invalid_json, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::server::ServerState;

use super::knowledge::present_string;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SubmitAction {
    Change,
    Create,
    Delete,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct SubmitRequest {
    action: SubmitAction,
    #[serde(default, deserialize_with = "present_string")]
    target_name: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    target_key: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    summary: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    content: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct PersonRequestPath {
    space: String,
    key: String,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskPath {
    space: String,
    #[serde(rename = "task")]
    task_id: Secret<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct ListQuery {
    status: Option<String>,
    keyword: Option<String>,
    page_size: Option<String>,
    cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct RejectRequest {
    reason: String,
    revision: i64,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct DecisionRequest {
    revision: i64,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviseRequest {
    #[serde(default, deserialize_with = "present_string")]
    name: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    summary: Option<String>,
    #[serde(default, deserialize_with = "present_string")]
    content: Option<String>,
}

pub(crate) async fn revise(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<PersonRequestPath>, PathRejection>,
    request: Result<Json<ReviseRequest>, JsonRejection>,
) -> Result<Envelope<RevisedPersonRequest>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let changes =
        if request.name.is_none() && request.summary.is_none() && request.content.is_none() {
            None
        } else {
            Some(KnowledgeChanges::new(
                request.name,
                request.summary,
                request.content,
            )?)
        };
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| {
            revise_person_request(write, &caller, &path.space, &path.key, changes).await
        })
        .await?;
    Ok(Envelope::builder().data(result).build())
}

impl TryFrom<SubmitRequest> for PersonRequestContent {
    type Error = ApiError;

    fn try_from(value: SubmitRequest) -> Result<Self, Self::Error> {
        Ok(match value.action {
            SubmitAction::Create => {
                refuse_fields(&[
                    ("target_name", &value.target_name),
                    ("target_key", &value.target_key),
                ])?;
                Self::Create(NewKnowledgeEntry::new(
                    required("name", value.name)?,
                    required("summary", value.summary)?,
                    required("content", value.content)?,
                )?)
            }
            SubmitAction::Change => Self::Change(
                KnowledgeTarget::new(value.target_name, value.target_key)?,
                KnowledgeChanges::new(value.name, value.summary, value.content)?,
            ),
            SubmitAction::Delete => {
                refuse_fields(&[
                    ("name", &value.name),
                    ("summary", &value.summary),
                    ("content", &value.content),
                ])?;
                Self::Delete(KnowledgeTarget::new(value.target_name, value.target_key)?)
            }
        })
    }
}

pub(crate) async fn submit_person(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<String>, PathRejection>,
    request: Result<Json<SubmitRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<SubmittedPersonRequest>), ApiError> {
    let Path(space) = path.map_err(|_| invalid_query("space"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let content = request.try_into()?;
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| {
            submit_person_request(write, &caller, PersonRequestSource::Space(&space), content).await
        })
        .await?;
    Ok((
        StatusCode::CREATED,
        Envelope::builder().data(result).build(),
    ))
}

pub(crate) async fn submit_agent(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<TaskPath>, PathRejection>,
    request: Result<Json<SubmitRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<SubmittedPersonRequest>), ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let content = request.try_into()?;
    let source = PersonRequestSource::Task(TaskTarget {
        path_space: Some(&path.space),
        task_id: path.task_id.expose(),
    });
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| submit_person_request(write, &caller, source, content).await)
        .await?;
    Ok((
        StatusCode::CREATED,
        Envelope::builder().data(result).build(),
    ))
}

pub(crate) async fn list(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<String>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Envelope<Vec<PersonRequestListItem>>, ApiError> {
    let Path(space) = path.map_err(|_| invalid_query("space"))?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let options = PageOptions::from_text(query.page_size.as_deref(), query.cursor)
        .map_err(|_| invalid_query("page_size"))?;
    let status = query
        .status
        .as_deref()
        .map(PersonRequestStatus::try_from)
        .transpose()
        .map_err(|_| invalid_query("status"))?;
    let filters = PersonRequestFilters::new(status, query.keyword)?;
    let page = list_person_requests(
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
    path: Result<Path<PersonRequestPath>, PathRejection>,
) -> Result<Envelope<PersonRequest>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let result = read_person_request(
        &state.library,
        &caller.library_caller(),
        &path.space,
        &path.key,
    )
    .await?;
    Ok(Envelope::builder().data(result).build())
}

pub(crate) async fn approve(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<PersonRequestPath>, PathRejection>,
    request: Result<Json<DecisionRequest>, JsonRejection>,
) -> Result<Envelope<PersonRequest>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| {
            approve_person_request(write, &caller, &path.space, &path.key, request.revision).await
        })
        .await?;
    Ok(Envelope::builder().data(result).build())
}

pub(crate) async fn reject(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<PersonRequestPath>, PathRejection>,
    request: Result<Json<RejectRequest>, JsonRejection>,
) -> Result<Envelope<PersonRequest>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let rejection = PersonRequestRejection::new(request.reason, request.revision)?;
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| {
            reject_person_request(write, &caller, &path.space, &path.key, rejection).await
        })
        .await?;
    Ok(Envelope::builder().data(result).build())
}

fn required(field: &str, value: Option<String>) -> Result<String, ApiError> {
    value.ok_or_else(|| invalid_field(field, "required", &format!("`{field}` is required.")))
}

fn refuse_fields(fields: &[(&str, &Option<String>)]) -> Result<(), ApiError> {
    if let Some((field, _)) = fields.iter().find(|(_, value)| value.is_some()) {
        return Err(invalid_field(
            field,
            "invalid",
            &format!("`{field}` is not accepted."),
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
