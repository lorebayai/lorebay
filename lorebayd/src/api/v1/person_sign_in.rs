use std::error::Error;
use std::net::SocketAddr;

use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use lorebay_lib_types::Secret;
use serde::{Deserialize, Serialize};
use serde_json::Error as JsonError;
use serde_path_to_error::{Error as PathError, Segment};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError};
use crate::domain::caller::VerifiedCaller;
use crate::domain::person_sign_in::{self, Tokens};
use crate::oauth::no_store_headers;
use crate::server::ServerState;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct LoginRequest {
    username: String,
    password: Secret<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct RefreshRequest {
    refresh_token: Secret<String>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TokenResponse {
    access_token: Secret<String>,
    refresh_token: Secret<String>,
    token_type: &'static str,
    expires_in: u64,
}

impl From<Tokens> for TokenResponse {
    fn from(tokens: Tokens) -> Self {
        Self::builder()
            .access_token(tokens.access_token)
            .refresh_token(tokens.refresh_token)
            .token_type("Bearer")
            .expires_in(tokens.expires_in)
            .build()
    }
}

impl LoginRequest {
    fn validate(&self) -> Result<(), ApiError> {
        validate_field("username", &self.username)?;
        validate_field("password", self.password.expose())
    }
}

pub(crate) async fn log_in(
    State(state): State<ServerState>,
    Extension(peer): Extension<SocketAddr>,
    request: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = request.map_err(invalid_json)?;
    request.validate()?;
    let tokens = person_sign_in::log_in(
        &state.identity,
        &state.database,
        peer.ip(),
        request.username,
        request.password,
    )
    .await?;
    Ok((
        no_store_headers(),
        Envelope::<TokenResponse>::builder().data(tokens).build(),
    )
        .into_response())
}

pub(crate) async fn refresh(
    State(state): State<ServerState>,
    request: Result<Json<RefreshRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = request.map_err(invalid_json)?;
    validate_field("refresh_token", request.refresh_token.expose())?;
    let tokens = person_sign_in::refresh(
        &state.identity,
        &state.database,
        request.refresh_token.expose(),
    )
    .await?;
    Ok((
        no_store_headers(),
        Envelope::<TokenResponse>::builder().data(tokens).build(),
    )
        .into_response())
}

pub(crate) async fn log_out(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
) -> Result<Envelope<()>, ApiError> {
    person_sign_in::log_out(&state.database, caller).await?;
    Ok(Envelope::builder().data(()).build())
}

pub(super) fn invalid_json(error: JsonRejection) -> ApiError {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return ApiError::PayloadTooLarge;
    }
    if let JsonRejection::JsonDataError(error) = &error {
        let mut source: &(dyn Error + 'static) = error;
        loop {
            if let Some(error) = source.downcast_ref::<PathError<JsonError>>() {
                if let Some(field) = json_field_error(error) {
                    return ApiError::InvalidBody(vec![field]);
                }
                break;
            }
            let Some(next) = source.source() else {
                break;
            };
            source = next;
        }
    }
    ApiError::invalid_body()
}

fn json_field_error(error: &PathError<JsonError>) -> Option<FieldError> {
    let message = error.inner().to_string();
    let named = |prefix| {
        message
            .strip_prefix(prefix)?
            .split_once('`')
            .map(|(name, _)| name)
    };
    let (field, code, message) = if let Some(field) = named("missing field `") {
        (field, "required", format!("`{field}` is required."))
    } else if let Some(field) = named("unknown field `") {
        (field, "invalid", format!("`{field}` is not accepted."))
    } else {
        let Segment::Map { key: field } = error.path().iter().next()? else {
            return None;
        };
        let expected = message
            .rsplit_once(", expected ")
            .map(|(_, expected)| expected)
            .and_then(|expected| {
                expected
                    .rsplit_once(" at line ")
                    .map(|(expected, _)| expected)
            })
            .unwrap_or("a valid value");
        let expected = match (field.as_str(), expected) {
            ("result_version", _) => "a positive integer or null",
            (_, "i64" | "u64") => "an integer",
            _ => expected,
        };
        (
            field.as_str(),
            "invalid",
            format!("`{field}` must be {expected}."),
        )
    };
    Some(
        FieldError::builder()
            .field(field)
            .code(code)
            .message(message)
            .build(),
    )
}

fn validate_field(field: &str, value: &str) -> Result<(), ApiError> {
    if value.is_empty() {
        return Err(ApiError::InvalidBody(vec![
            FieldError::builder()
                .field(field)
                .code("required")
                .message("Must not be empty.")
                .build(),
        ]));
    }
    Ok(())
}
