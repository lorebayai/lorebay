use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, ListQuery, Meta, invalid_query};
use crate::domain::caller::VerifiedCaller;
use crate::domain::{UserDirectoryError, list_users};
use crate::server::ServerState;

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct UserResponse {
    username: String,
}

impl From<UserDirectoryError> for ApiError {
    fn from(error: UserDirectoryError) -> Self {
        match error {
            UserDirectoryError::Caller(error) => error.into(),
            UserDirectoryError::InvalidCursor => invalid_query("cursor"),
            error @ UserDirectoryError::Database(_) => {
                tracing::error!(%error, "user list failed");
                Self::Internal
            }
        }
    }
}

pub(crate) async fn list(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Envelope<Vec<UserResponse>>, ApiError> {
    let Query(query) = query.map_err(|_| invalid_query("query"))?;
    let page = list_users(&state.database, &caller, query.try_into()?).await?;
    Ok(Envelope::builder()
        .data(
            page.items
                .into_iter()
                .map(|username| UserResponse::builder().username(username).build())
                .collect::<Vec<_>>(),
        )
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build())
}
