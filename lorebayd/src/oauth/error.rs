use axum::Json;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use lorebay_base::RateLimitError;
use serde::Serialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::domain::OAuthClientError;
use crate::domain::device_sign_in::DeviceError;
use crate::domain::oauth_token::TokenError;

#[derive(Debug, Error)]
pub(super) enum OAuthError {
    #[error("device sign-in request failed")]
    Device(&'static str, &'static str),
    #[error("device request rate limit exceeded")]
    DeviceRateLimited(u64),
    #[error("internal server error")]
    Internal,
    #[error("invalid client metadata")]
    InvalidClientMetadata,
    #[error("invalid device authorization request")]
    InvalidDeviceRequest(StatusCode),
    #[error("invalid authorization grant")]
    InvalidGrant,
    #[error("invalid redirect URI")]
    InvalidRedirectUri,
    #[error("invalid registration request")]
    InvalidRequest(StatusCode),
    #[error("invalid resource")]
    InvalidTarget,
    #[error("invalid token request")]
    InvalidTokenRequest(StatusCode),
    #[error("method not allowed")]
    MethodNotAllowed,
    #[error("registration rate limit exceeded")]
    RateLimited(u64),
    #[error("unsupported grant type")]
    UnsupportedGrantType,
}

impl From<DeviceError> for OAuthError {
    fn from(error: DeviceError) -> Self {
        match error {
            DeviceError::AccessDenied => {
                Self::Device("access_denied", "The person declined this request.")
            }
            DeviceError::AuthorizationPending => {
                Self::Device("authorization_pending", "The person has not decided yet.")
            }
            DeviceError::ExpiredToken => {
                Self::Device("expired_token", "The device sign-in request has expired.")
            }
            DeviceError::InvalidClient => Self::Device(
                "unauthorized_client",
                "Only the built-in CLI client may use device sign-in.",
            ),
            DeviceError::InvalidGrant => Self::InvalidGrant,
            DeviceError::RateLimit(error) => match Self::from(error) {
                Self::RateLimited(seconds) => Self::DeviceRateLimited(seconds),
                error => error,
            },
            DeviceError::SlowDown => Self::Device(
                "slow_down",
                "Increase the polling interval by five seconds.",
            ),
            error => {
                tracing::error!(%error, "device sign-in operation failed");
                Self::Internal
            }
        }
    }
}

impl From<TokenError> for OAuthError {
    fn from(error: TokenError) -> Self {
        match error {
            TokenError::Device(error) => error.into(),
            TokenError::AgentNameTaken | TokenError::InvalidGrant => Self::InvalidGrant,
            error => {
                tracing::error!(%error, "OAuth token operation failed");
                Self::Internal
            }
        }
    }
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

impl OAuthError {
    fn details(&self) -> (StatusCode, &'static str, &'static str) {
        match *self {
            Self::DeviceRateLimited(_) => device_rate_limit_details(),
            Self::Device(code, description) => (StatusCode::BAD_REQUEST, code, description),
            Self::InvalidDeviceRequest(status) => (
                status,
                "invalid_request",
                "Expected a form-encoded device authorization request with client_id.",
            ),
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
            Self::InvalidGrant => (
                StatusCode::BAD_REQUEST,
                "invalid_grant",
                "The authorization grant is invalid, expired or belongs to another client.",
            ),
            Self::InvalidRequest(status) => (
                status,
                "invalid_client_metadata",
                "Expected a JSON client registration object.",
            ),
            Self::InvalidTarget => (
                StatusCode::BAD_REQUEST,
                "invalid_target",
                "The resource must be the MCP endpoint URL or the public URL.",
            ),
            Self::InvalidTokenRequest(status) => (
                status,
                "invalid_request",
                "Expected a form-encoded token request with the required parameters.",
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
            Self::UnsupportedGrantType => (
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                "The grant type is not supported.",
            ),
        }
    }
}

impl IntoResponse for OAuthError {
    fn into_response(self) -> Response {
        let (status, code, description) = self.details();
        let body = ErrorResponse::builder()
            .error(code)
            .error_description(description)
            .build();
        let mut response = (status, no_store_headers(), Json(body)).into_response();
        if let Self::RateLimited(seconds) | Self::DeviceRateLimited(seconds) = self
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

fn device_rate_limit_details() -> (StatusCode, &'static str, &'static str) {
    (
        StatusCode::TOO_MANY_REQUESTS,
        "slow_down",
        "Too many device sign-in attempts; retry after the indicated wait.",
    )
}
