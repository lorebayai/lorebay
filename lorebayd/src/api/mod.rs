mod envelope;
mod problem;

use axum::Router;
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::http::StatusCode;

pub(crate) use envelope::*;
pub(crate) use problem::*;

pub(crate) fn router() -> Router {
    Router::new().fallback(not_found)
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
        Err(_) => ApiError::Internal,
    }
}
