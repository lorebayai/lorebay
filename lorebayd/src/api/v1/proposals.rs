use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use lorebay_lib_core::{
    KnowledgeChanges, KnowledgeTarget, NewKnowledgeEntry, PageOptions, Proposal, ProposalContent,
    ProposalFilters, ProposalListItem, ProposalRejection, ProposalSource, ProposalStatus,
    SubmittedProposal, TaskTarget, approve_proposal, list_proposals, read_proposal,
    reject_proposal, submit_proposal,
};
use lorebay_lib_types::Secret;
use serde::Deserialize;
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, Meta, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::server::ServerState;

use super::knowledge::present_string;
use super::login_session::invalid_json;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub(crate) enum SubmitRequest {
    Change {
        target_name: Option<String>,
        target_key: Option<String>,
        #[serde(default, deserialize_with = "present_string")]
        name: Option<String>,
        #[serde(default, deserialize_with = "present_string")]
        summary: Option<String>,
        #[serde(default, deserialize_with = "present_string")]
        content: Option<String>,
    },
    Create {
        name: String,
        summary: String,
        content: String,
    },
    Delete {
        target_name: Option<String>,
        target_key: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ProposalPath {
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
}

impl TryFrom<SubmitRequest> for ProposalContent {
    type Error = ApiError;

    fn try_from(value: SubmitRequest) -> Result<Self, Self::Error> {
        Ok(match value {
            SubmitRequest::Create {
                name,
                summary,
                content,
            } => Self::Create(NewKnowledgeEntry::new(name, summary, content)?),
            SubmitRequest::Change {
                target_name,
                target_key,
                name,
                summary,
                content,
            } => Self::Change(
                KnowledgeTarget::new(target_name, target_key)?,
                KnowledgeChanges::new(name, summary, content)?,
            ),
            SubmitRequest::Delete {
                target_name,
                target_key,
            } => Self::Delete(KnowledgeTarget::new(target_name, target_key)?),
        })
    }
}

pub(crate) async fn submit_person(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<String>, PathRejection>,
    request: Result<Json<SubmitRequest>, JsonRejection>,
) -> Result<(StatusCode, Envelope<SubmittedProposal>), ApiError> {
    let Path(space) = path.map_err(|_| invalid_query("space"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let content = request.try_into()?;
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| {
            submit_proposal(write, &caller, ProposalSource::Space(&space), content).await
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
) -> Result<(StatusCode, Envelope<SubmittedProposal>), ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let content = request.try_into()?;
    let source = ProposalSource::Task(TaskTarget {
        path_space: Some(&path.space),
        task_id: path.task_id.expose(),
    });
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| submit_proposal(write, &caller, source, content).await)
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
) -> Result<Envelope<Vec<ProposalListItem>>, ApiError> {
    let Path(space) = path.map_err(|_| invalid_query("space"))?;
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let options = PageOptions::from_text(query.page_size.as_deref(), query.cursor)
        .map_err(|_| invalid_query("page_size"))?;
    let status = query
        .status
        .as_deref()
        .map(ProposalStatus::try_from)
        .transpose()
        .map_err(|_| invalid_query("status"))?;
    let filters = ProposalFilters::new(status, query.keyword)?;
    let page = list_proposals(
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
    path: Result<Path<ProposalPath>, PathRejection>,
) -> Result<Envelope<Proposal>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let result = read_proposal(
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
    path: Result<Path<ProposalPath>, PathRejection>,
) -> Result<Envelope<Proposal>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| approve_proposal(write, &caller, &path.space, &path.key).await)
        .await?;
    Ok(Envelope::builder().data(result).build())
}

pub(crate) async fn reject(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    path: Result<Path<ProposalPath>, PathRejection>,
    request: Result<Json<RejectRequest>, JsonRejection>,
) -> Result<Envelope<Proposal>, ApiError> {
    let Path(path) = path.map_err(|_| invalid_query("path"))?;
    let Json(request) = request.map_err(invalid_json)?;
    let rejection = ProposalRejection::new(request.reason)?;
    let caller = caller.library_caller();
    let result = state
        .library
        .run_write(async |write| {
            reject_proposal(write, &caller, &path.space, &path.key, rejection).await
        })
        .await?;
    Ok(Envelope::builder().data(result).build())
}
