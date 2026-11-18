use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use super::invalid_query;
use crate::domain::access::AccessError;
use crate::domain::caller::CallerError;
use crate::domain::login_session::LoginError;
use lorebay_lib_core::{
    ArtifactError, CallerError as LibraryCallerError, SpaceAccessError, SpaceError,
    SpaceInputError, TaskAccessError, TaskError,
};

#[derive(Debug, Error)]
pub(crate) enum ApiError {
    #[error("An agent authorization is required")]
    AgentRequired,
    #[error("Artifact not found")]
    ArtifactNotFound,
    #[error("The artifact file has not been uploaded")]
    ArtifactNotUploaded,
    #[error("The artifact file exceeds the size limit")]
    ArtifactTooLarge,
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
    #[error("Note not found")]
    NoteNotFound,
    #[error("The person is already in the space")]
    ParticipantAlreadyExists,
    #[error("Participant not found")]
    ParticipantNotFound,
    #[error("Payload Too Large")]
    PayloadTooLarge,
    #[error("A login session is required")]
    PersonRequired,
    #[error("Too Many Requests")]
    RateLimited(u64),
    #[error("Space not found")]
    SpaceNotFound,
    #[error("The space role does not allow this operation")]
    SpacePermissionRequired,
    #[error("Set the task back to in progress before {0}")]
    TaskEnded(&'static str),
    #[error("Task not found")]
    TaskNotFound,
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Service Unavailable")]
    Unavailable,
    #[error("User not found")]
    UserNotFound,
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
    pub(crate) fn invalid_body() -> Self {
        Self::InvalidBody(vec![
            FieldError::builder()
                .field("body")
                .code("invalid")
                .message("Expected a JSON object with the required string fields.")
                .build(),
        ])
    }

    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            Self::AgentRequired => (StatusCode::FORBIDDEN, "auth.agent_required"),
            Self::ArtifactNotFound => (StatusCode::NOT_FOUND, "artifact.not_found"),
            Self::ArtifactNotUploaded => (StatusCode::CONFLICT, "artifact.not_uploaded"),
            Self::ArtifactTooLarge => (StatusCode::CONFLICT, "artifact.too_large"),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "server.internal"),
            Self::InvalidBody(_) => (StatusCode::BAD_REQUEST, "request.invalid_body"),
            Self::InvalidPath(_) => (StatusCode::BAD_REQUEST, "request.invalid_path"),
            Self::InvalidQuery(_) => (StatusCode::BAD_REQUEST, "request.invalid_query"),
            Self::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "http.method_not_allowed"),
            Self::NotFound => (StatusCode::NOT_FOUND, "http.not_found"),
            Self::NoteNotFound => (StatusCode::NOT_FOUND, "note.not_found"),
            Self::ParticipantAlreadyExists => (StatusCode::CONFLICT, "participant.already_exists"),
            Self::ParticipantNotFound => (StatusCode::NOT_FOUND, "participant.not_found"),
            Self::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "request.body_too_large"),
            Self::PersonRequired => (StatusCode::FORBIDDEN, "auth.person_required"),
            Self::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, "auth.rate_limited"),
            Self::SpaceNotFound => (StatusCode::NOT_FOUND, "space.not_found"),
            Self::SpacePermissionRequired => (StatusCode::FORBIDDEN, "space.permission_required"),
            Self::TaskEnded(_) => (StatusCode::CONFLICT, "task.ended"),
            Self::TaskNotFound => (StatusCode::NOT_FOUND, "task.not_found"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "auth.unauthorized"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "server.not_ready"),
            Self::UserNotFound => (StatusCode::NOT_FOUND, "user.not_found"),
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

impl From<SpaceAccessError> for ApiError {
    fn from(error: SpaceAccessError) -> Self {
        match error {
            SpaceAccessError::NotFound => Self::SpaceNotFound,
            SpaceAccessError::PermissionRequired => Self::SpacePermissionRequired,
            error @ (SpaceAccessError::Database(_) | SpaceAccessError::Internal(_)) => {
                tracing::error!(%error, "space access check failed");
                Self::Internal
            }
        }
    }
}

impl From<TaskAccessError> for ApiError {
    fn from(error: TaskAccessError) -> Self {
        match error {
            error @ (TaskAccessError::Database(_) | TaskAccessError::Internal(_)) => {
                tracing::error!(%error, "task access check failed");
                Self::Internal
            }
            TaskAccessError::Ended(operation) => Self::TaskEnded(operation),
            TaskAccessError::NotFound => Self::TaskNotFound,
            TaskAccessError::PermissionRequired => Self::SpacePermissionRequired,
            TaskAccessError::Space(error) => error.into(),
        }
    }
}

impl From<LibraryCallerError> for ApiError {
    fn from(error: LibraryCallerError) -> Self {
        match error {
            LibraryCallerError::AgentRequired => Self::AgentRequired,
            LibraryCallerError::PersonRequired => Self::PersonRequired,
        }
    }
}

impl From<ArtifactError> for ApiError {
    fn from(error: ArtifactError) -> Self {
        match error {
            ArtifactError::Access(error) => error.into(),
            ArtifactError::Caller(error) => error.into(),
            ArtifactError::Database(_) | ArtifactError::Internal(_) => {
                tracing::error!(%error, "artifact operation failed");
                Self::Internal
            }
            ArtifactError::NotFound => Self::ArtifactNotFound,
            ArtifactError::NotUploaded => Self::ArtifactNotUploaded,
            ArtifactError::Space(error) => error.into(),
            ArtifactError::TooLarge => Self::ArtifactTooLarge,
        }
    }
}

impl From<SpaceError> for ApiError {
    fn from(error: SpaceError) -> Self {
        match error {
            SpaceError::Access(error) => error.into(),
            SpaceError::Caller(error) => error.into(),
            SpaceError::Input(error) => error.into(),
            SpaceError::InvalidCursor => invalid_query("cursor"),
            SpaceError::InvalidPageSize => invalid_query("page_size"),
            SpaceError::Database(_) | SpaceError::Internal(_) | SpaceError::NumberTaken => {
                tracing::error!(%error, "space operation failed");
                Self::Internal
            }
        }
    }
}

impl From<SpaceInputError> for ApiError {
    fn from(error: SpaceInputError) -> Self {
        Self::InvalidBody(vec![FieldError {
            field: error.field().to_owned(),
            code: error.field_code().to_owned(),
            message: error.to_string(),
        }])
    }
}

impl From<TaskError> for ApiError {
    fn from(error: TaskError) -> Self {
        match error {
            TaskError::Access(error) => error.into(),
            TaskError::Caller(error) => error.into(),
            TaskError::Database(_) | TaskError::Internal(_) => {
                tracing::error!(%error, "task operation failed");
                Self::Internal
            }
            TaskError::Filter(error) => Self::InvalidQuery(vec![FieldError {
                field: error.field().to_owned(),
                code: "invalid".to_owned(),
                message: error.to_string(),
            }]),
            TaskError::InvalidCursor => invalid_query("cursor"),
            TaskError::InvalidPageSize => invalid_query("page_size"),
            TaskError::NoChanges => Self::InvalidBody(vec![FieldError {
                field: "body".to_owned(),
                code: "required".to_owned(),
                message: error.to_string(),
            }]),
            TaskError::NotFound => Self::TaskNotFound,
            TaskError::NoteNotFound => Self::NoteNotFound,
            TaskError::Space(error) => error.into(),
        }
    }
}

impl From<ApiError> for ProblemDetails {
    fn from(error: ApiError) -> Self {
        let (status, code) = error.parts();
        let title = error.to_string();
        let errors = match error {
            ApiError::InvalidBody(errors)
            | ApiError::InvalidPath(errors)
            | ApiError::InvalidQuery(errors) => errors,
            _ => Vec::new(),
        };
        Self::builder()
            .problem_type(GENERIC_PROBLEM_TYPE)
            .title(title)
            .status(status.as_u16())
            .code(code)
            .errors(errors)
            .build()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, _) = self.parts();
        let wait = match &self {
            Self::RateLimited(wait) => Some(*wait),
            _ => None,
        };
        let problem = ProblemDetails::from(self);
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
