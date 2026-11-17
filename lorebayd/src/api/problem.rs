use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::domain::access::AccessError;
use crate::domain::caller::CallerError;
use crate::domain::login_session::LoginError;

#[derive(Debug, Error)]
pub(crate) enum ApiError {
    #[error("Internal Server Error")]
    Internal,
    #[error("Bad Request")]
    InvalidBody(Vec<FieldError>),
    #[error("Bad Request")]
    InvalidPath(Vec<FieldError>),
    #[error("Bad Request")]
    InvalidQuery(Vec<FieldError>),
    #[error("Method Not Allowed")]
    MethodNotAllowed,
    #[error("Not Found")]
    NotFound,
    #[error("Payload Too Large")]
    PayloadTooLarge,
    #[error("A login session is required")]
    PersonRequired,
    #[error("Too Many Requests")]
    RateLimited(u64),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Service Unavailable")]
    Unavailable,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct FieldError {
    pub field: String,
    pub code: String,
    pub message: String,
}

/// Internal causes have no field in the wire representation.
#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: &'static str,
    pub title: String,
    pub status: u16,
    pub code: &'static str,
    pub errors: Vec<FieldError>,
}

impl ApiError {
    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "server.internal"),
            Self::InvalidBody(_) => (StatusCode::BAD_REQUEST, "request.invalid_body"),
            Self::InvalidPath(_) => (StatusCode::BAD_REQUEST, "request.invalid_path"),
            Self::InvalidQuery(_) => (StatusCode::BAD_REQUEST, "request.invalid_query"),
            Self::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "http.method_not_allowed"),
            Self::NotFound => (StatusCode::NOT_FOUND, "http.not_found"),
            Self::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "request.body_too_large"),
            Self::PersonRequired => (StatusCode::FORBIDDEN, "auth.person_required"),
            Self::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, "auth.rate_limited"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "auth.unauthorized"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "server.not_ready"),
        }
    }
}

impl From<AccessError> for ApiError {
    fn from(error: AccessError) -> Self {
        match error {
            AccessError::Unauthorized => Self::Unauthorized,
            error => {
                tracing::error!(%error, "caller verification failed");
                Self::Internal
            }
        }
    }
}

impl From<CallerError> for ApiError {
    fn from(error: CallerError) -> Self {
        match error {
            CallerError::PersonRequired => Self::PersonRequired,
        }
    }
}

impl From<LoginError> for ApiError {
    fn from(error: LoginError) -> Self {
        match error {
            LoginError::Caller(error) => error.into(),
            LoginError::RateLimited(wait) => Self::RateLimited(wait),
            LoginError::Unauthorized => Self::Unauthorized,
            error => {
                tracing::error!(%error, "login session operation failed");
                Self::Internal
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = self.parts();
        let title = self.to_string();
        let wait = match &self {
            Self::RateLimited(wait) => Some(*wait),
            _ => None,
        };
        let errors = match self {
            Self::InvalidBody(errors) | Self::InvalidPath(errors) | Self::InvalidQuery(errors) => {
                errors
            }
            _ => Vec::new(),
        };
        let problem = ProblemDetails::builder()
            .problem_type(GENERIC_PROBLEM_TYPE)
            .title(title)
            .status(status.as_u16())
            .code(code)
            .errors(errors)
            .build();
        let mut response = (
            status,
            [(header::CONTENT_TYPE, PROBLEM_MEDIA_TYPE)],
            Json(problem),
        )
            .into_response();
        if let Some(wait) = wait
            && let Ok(value) = wait.to_string().parse()
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        if status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        response
    }
}

const PROBLEM_MEDIA_TYPE: &str = "application/problem+json";
const GENERIC_PROBLEM_TYPE: &str = "about:blank";
