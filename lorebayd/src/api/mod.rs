use crate::server::ServerState;
use axum::Router;
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::http::StatusCode;

pub(crate) use envelope::*;
pub(crate) use pagination::*;
pub(crate) use problem::*;
pub(crate) use v1::{
    NoteAddedResponse, ReadTaskQuery, SpaceResponse, TaskResponse, TaskSummaryResponse,
    invalid_text,
};

pub(crate) fn router() -> Router<ServerState> {
    v1::router().fallback(not_found)
}

pub(crate) async fn not_found(body: Result<Bytes, BytesRejection>) -> ApiError {
    match body {
        Ok(_) => ApiError::NotFound,
        Err(error) if error.status() == StatusCode::PAYLOAD_TOO_LARGE => ApiError::PayloadTooLarge,
        Err(error) if error.status().is_client_error() => ApiError::InvalidBody(vec![
            FieldError::builder()
                .field("body")
                .code("invalid")
                .message("Could not read request body.")
                .build(),
        ]),
        Err(error) => {
            tracing::error!(%error, "could not read request body");
            ApiError::Internal
        }
    }
}

mod auth;
mod envelope;
mod pagination;
mod problem;
mod v1;
