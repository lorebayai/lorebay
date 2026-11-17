use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, ListQuery, Meta, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::domain::agents::{self, Agent, AgentError};
use crate::server::ServerState;

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AgentResponse {
    name: String,
    client_id: String,
    created_at: i64,
}

impl From<Agent> for AgentResponse {
    fn from(agent: Agent) -> Self {
        Self::builder()
            .name(agent.name)
            .client_id(agent.client_id)
            .created_at(agent.created_at)
            .build()
    }
}

impl From<AgentError> for ApiError {
    fn from(error: AgentError) -> Self {
        match error {
            AgentError::Caller(error) => error.into(),
            AgentError::InvalidCursor => invalid_query("cursor"),
            AgentError::NotFound => Self::NotFound,
            error @ AgentError::Database(_) => {
                tracing::error!(%error, "agent operation failed");
                Self::Internal
            }
        }
    }
}

pub(crate) async fn list(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Envelope<Vec<AgentResponse>>, ApiError> {
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let page = agents::list(&state.database, &caller, query.try_into()?).await?;
    Ok(Envelope::builder()
        .data(
            page.items
                .into_iter()
                .map(AgentResponse::from)
                .collect::<Vec<_>>(),
        )
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build())
}

pub(crate) async fn revoke(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    name: Result<Path<String>, PathRejection>,
) -> Result<Envelope<()>, ApiError> {
    let Path(name) = name.map_err(|_| invalid_query("name"))?;
    agents::revoke(&state.database, &caller, &name).await?;
    Ok(Envelope::builder().data(()).build())
}
