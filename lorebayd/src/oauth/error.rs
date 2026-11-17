use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use lorebay_base::RateLimitError;
use serde::Serialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::domain::OAuthClientError;

#[derive(Debug, Error)]
pub(super) enum OAuthError {
    #[error("internal server error")]
    Internal,
    #[error("invalid client metadata")]
    InvalidClientMetadata,
    #[error("invalid redirect URI")]
    InvalidRedirectUri,
    #[error("invalid registration request")]
    InvalidRequest(StatusCode),
    #[error("method not allowed")]
    MethodNotAllowed,
    #[error("registration rate limit exceeded")]
    RateLimited(u64),
}

impl From<OAuthClientError> for OAuthError {
    fn from(error: OAuthClientError) -> Self {
        tracing::error!(%error, "could not register OAuth client");
        Self::Internal
    }
}

impl From<RateLimitError> for OAuthError {
    fn from(error: RateLimitError) -> Self {
        match error {
            RateLimitError::Exceeded { retry_after } => Self::RateLimited(
                retry_after
                    .as_secs()
                    .saturating_add(u64::from(retry_after.subsec_nanos() > 0))
                    .max(1),
            ),
            RateLimitError::Unavailable => {
                tracing::error!(%error, "could not check registration rate limit");
                Self::Internal
            }
        }
    }
}

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        let (status, code, description) = match self {
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "An internal error occurred.",
            ),
            Self::InvalidClientMetadata => (
                StatusCode::BAD_REQUEST,
                "invalid_client_metadata",
                "Unsupported or invalid client metadata.",
            ),
            Self::InvalidRedirectUri => (
                StatusCode::BAD_REQUEST,
                "invalid_redirect_uri",
                "Redirect URIs must use HTTPS or HTTP on loopback, without credentials or fragments.",
            ),
            Self::InvalidRequest(status) => (
                status,
                "invalid_client_metadata",
                "Expected a JSON client registration object.",
            ),
            Self::MethodNotAllowed => (
                StatusCode::METHOD_NOT_ALLOWED,
                "invalid_request",
                "The request method is not supported.",
            ),
            Self::RateLimited(_) => (
                StatusCode::TOO_MANY_REQUESTS,
                "slow_down",
                "Too many registration attempts; retry after the indicated wait.",
            ),
        };
        let body = ErrorResponse::builder()
            .error(code)
            .error_description(description)
            .build();
        let mut response = (status, no_store_headers(), Json(body)).into_response();
        if let Self::RateLimited(seconds) = self
            && let Ok(value) = seconds.to_string().parse()
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}

pub(crate) fn no_store_headers() -> [(header::HeaderName, &'static str); 2] {
    [
        (header::CACHE_CONTROL, "no-store"),
        (header::PRAGMA, "no-cache"),
    ]
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct ErrorResponse {
    error: &'static str,
    error_description: &'static str,
}
