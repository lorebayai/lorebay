use std::fmt::Display;

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use super::invalid_query;
use crate::domain::access::AccessError;
use crate::domain::caller::PersonCallerError;
use crate::domain::person_sign_in::LoginError;
use lorebay_lib_core::ProposalError;
use lorebay_lib_core::{
    AgentFileError, CallerError as LibraryCallerError, SpaceAccessError, SpaceError,
    SpaceInputError, TaskAccessError, TaskError,
};
use lorebay_lib_core::{KnowledgeError, KnowledgeInputError};
use lorebay_lib_core::{PageSizeError, TaskChangeError};

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
    #[error("{message}")]
    Library {
        status: StatusCode,
        code: &'static str,
        message: String,
        errors: Vec<FieldError>,
    },
    #[error("Method Not Allowed")]
    MethodNotAllowed,
    #[error("Not Found")]
    NotFound,
    #[error("Payload Too Large")]
    PayloadTooLarge,
    #[error("A person's sign-in is required")]
    PersonRequired,
    #[error("Too Many Requests")]
    RateLimited(u64),
    #[error("The person is already in the space")]
    SpaceMemberAlreadyExists,
    #[error("Space member not found")]
    SpaceMemberNotFound,
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

    fn library(status: StatusCode, code: &'static str, error: impl Display) -> Self {
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(%error, "library operation failed");
            "Internal Server Error".to_owned()
        } else {
            error.to_string()
        };
        Self::Library {
            status,
            code,
            message,
            errors: Vec::new(),
        }
    }

    fn with_library_code(self, code: &'static str) -> Self {
        let status = self.parts().0;
        let message = self.to_string();
        let errors = match self {
            Self::InvalidBody(errors) | Self::InvalidQuery(errors) => errors,
            _ => Vec::new(),
        };
        Self::Library {
            status,
            code,
            message,
            errors,
        }
    }

    fn parts(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "server.internal"),
            Self::InvalidBody(_) => (StatusCode::BAD_REQUEST, "request.invalid_body"),
            Self::InvalidPath(_) => (StatusCode::BAD_REQUEST, "request.invalid_path"),
            Self::InvalidQuery(_) => (StatusCode::BAD_REQUEST, "request.invalid_query"),
            Self::Library { status, code, .. } => (*status, code),
            Self::MethodNotAllowed => (StatusCode::METHOD_NOT_ALLOWED, "http.method_not_allowed"),
            Self::NotFound => (StatusCode::NOT_FOUND, "http.not_found"),
            Self::SpaceMemberAlreadyExists => (StatusCode::CONFLICT, "space_member.already_exists"),
            Self::SpaceMemberNotFound => (StatusCode::NOT_FOUND, "space_member.not_found"),
            Self::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "request.body_too_large"),
            Self::PersonRequired => (StatusCode::FORBIDDEN, "auth.person_required"),
            Self::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, "auth.rate_limited"),
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

impl From<PersonCallerError> for ApiError {
    fn from(error: PersonCallerError) -> Self {
        match error {
            PersonCallerError::PersonRequired => Self::PersonRequired,
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
                tracing::error!(%error, "sign-in operation failed");
                Self::Internal
            }
        }
    }
}

impl From<SpaceAccessError> for ApiError {
    fn from(error: SpaceAccessError) -> Self {
        let status = match error {
            SpaceAccessError::NotFound => StatusCode::NOT_FOUND,
            SpaceAccessError::PermissionRequired => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::library(status, error.code(), error)
    }
}

impl From<TaskAccessError> for ApiError {
    fn from(error: TaskAccessError) -> Self {
        let status = match error {
            TaskAccessError::Ended(_) => StatusCode::CONFLICT,
            TaskAccessError::NotFound => StatusCode::NOT_FOUND,
            TaskAccessError::PermissionRequired => StatusCode::FORBIDDEN,
            TaskAccessError::Space(error) => return error.into(),
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::library(status, error.code(), error)
    }
}

impl From<LibraryCallerError> for ApiError {
    fn from(error: LibraryCallerError) -> Self {
        let message = match error {
            LibraryCallerError::AgentRequired => "An agent sign-in is required",
            LibraryCallerError::PersonRequired => "A person's sign-in is required",
            _ => return Self::library(StatusCode::INTERNAL_SERVER_ERROR, error.code(), error),
        };
        Self::library(StatusCode::FORBIDDEN, error.code(), message)
    }
}

impl From<AgentFileError> for ApiError {
    fn from(error: AgentFileError) -> Self {
        let status = match error {
            AgentFileError::Access(error) => return error.into(),
            AgentFileError::Caller(error) => return error.into(),
            AgentFileError::NotFound => StatusCode::NOT_FOUND,
            AgentFileError::NotUploaded | AgentFileError::Refused | AgentFileError::TooLarge => {
                StatusCode::CONFLICT
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::library(status, error.code(), error)
    }
}

impl From<SpaceError> for ApiError {
    fn from(error: SpaceError) -> Self {
        let code = error.code();
        let status = match error {
            SpaceError::Access(error) => return Self::from(error),
            SpaceError::Caller(error) => return Self::from(error),
            SpaceError::Input(error) => return Self::from(error).with_library_code(code),
            SpaceError::InvalidCursor => return invalid_query("cursor").with_library_code(code),
            SpaceError::InvalidPageSize(_) => {
                return invalid_query(PageSizeError::FIELD).with_library_code(code);
            }
            SpaceError::NumberTaken => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::library(status, code, error)
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

impl From<KnowledgeInputError> for ApiError {
    fn from(error: KnowledgeInputError) -> Self {
        Self::InvalidBody(vec![FieldError {
            field: error.field().to_owned(),
            code: "invalid".to_owned(),
            message: error.to_string(),
        }])
    }
}

impl From<KnowledgeError> for ApiError {
    fn from(error: KnowledgeError) -> Self {
        let status = match error {
            KnowledgeError::Access(error) => return error.into(),
            KnowledgeError::Caller(error) => return error.into(),
            KnowledgeError::Input(error) => return error.into(),
            KnowledgeError::InvalidCursor => return invalid_query("cursor"),
            KnowledgeError::InvalidFilter => StatusCode::BAD_REQUEST,
            KnowledgeError::InvalidPageSize(_) => return invalid_query("page_size"),
            KnowledgeError::NameTaken { .. } => StatusCode::CONFLICT,
            KnowledgeError::NotFound | KnowledgeError::VersionNotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::library(status, error.code(), error)
    }
}

impl From<ProposalError> for ApiError {
    fn from(error: ProposalError) -> Self {
        let status = match error {
            ProposalError::Access(error) => return error.into(),
            ProposalError::Caller(error) => return error.into(),
            ProposalError::Input(error) => return error.into(),
            ProposalError::Knowledge(error) => return error.into(),
            ProposalError::Task(error) => return error.into(),
            ProposalError::InvalidCursor => return invalid_query("cursor"),
            ProposalError::InvalidFilter => return invalid_query("keyword"),
            ProposalError::InvalidPageSize(_) => return invalid_query("page_size"),
            ProposalError::InvalidReason => {
                return Self::InvalidBody(vec![FieldError {
                    field: "reason".to_owned(),
                    code: "invalid".to_owned(),
                    message: error.to_string(),
                }]);
            }
            ProposalError::Decided => StatusCode::CONFLICT,
            ProposalError::NameTaken => StatusCode::CONFLICT,
            ProposalError::NotFound => StatusCode::NOT_FOUND,
            ProposalError::OwnParty => StatusCode::FORBIDDEN,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::library(status, error.code(), error)
    }
}

impl From<TaskChangeError> for ApiError {
    fn from(error: TaskChangeError) -> Self {
        Self::InvalidBody(vec![FieldError {
            field: TaskChangeError::FIELD.to_owned(),
            code: TaskChangeError::FIELD_CODE.to_owned(),
            message: error.to_string(),
        }])
    }
}

impl From<TaskError> for ApiError {
    fn from(error: TaskError) -> Self {
        let code = error.code();
        let status = match error {
            TaskError::Access(error) => return error.into(),
            TaskError::Caller(error) => return error.into(),
            TaskError::Filter(error) => {
                return Self::InvalidQuery(vec![FieldError {
                    field: error.field().to_owned(),
                    code: "invalid".to_owned(),
                    message: error.to_string(),
                }])
                .with_library_code(code);
            }
            TaskError::InvalidCursor => return invalid_query("cursor").with_library_code(code),
            TaskError::InvalidPageSize(_) => {
                return invalid_query(PageSizeError::FIELD).with_library_code(code);
            }
            TaskError::NoChanges(error) => return Self::from(error).with_library_code(code),
            TaskError::NotFound | TaskError::CheckpointNotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::library(status, code, error)
    }
}

impl From<ApiError> for ProblemDetails {
    fn from(error: ApiError) -> Self {
        let (status, code) = error.parts();
        let title = error.to_string();
        let errors = match error {
            ApiError::InvalidBody(errors)
            | ApiError::InvalidPath(errors)
            | ApiError::InvalidQuery(errors)
            | ApiError::Library { errors, .. } => errors,
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
