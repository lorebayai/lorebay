use std::net::SocketAddr;

use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::{Extension, Json};
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError};
use crate::domain::caller::VerifiedCaller;
use crate::domain::login_session::{self, Tokens};
use crate::server::ServerState;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
#[serde(deny_unknown_fields)]
pub(crate) struct RefreshRequest {
    refresh_token: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TokenResponse {
    access_token: String,
    refresh_token: String,
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

pub(crate) async fn log_in(
    State(state): State<ServerState>,
    Extension(peer): Extension<SocketAddr>,
    request: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Envelope<TokenResponse>, ApiError> {
    let Json(request) = request.map_err(invalid_json)?;
    request.validate()?;
    let tokens = login_session::log_in(
        &state.identity,
        &state.database,
        peer.ip(),
        request.username,
        request.password,
    )
    .await?;
    Ok(Envelope::builder().data(tokens).build())
}

pub(crate) async fn refresh(
    State(state): State<ServerState>,
    request: Result<Json<RefreshRequest>, JsonRejection>,
) -> Result<Envelope<TokenResponse>, ApiError> {
    let Json(request) = request.map_err(invalid_json)?;
    validate_field("refresh_token", &request.refresh_token)?;
    let tokens =
        login_session::refresh(&state.identity, &state.database, &request.refresh_token).await?;
    Ok(Envelope::builder().data(tokens).build())
}

pub(crate) async fn log_out(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
) -> Result<Envelope<()>, ApiError> {
    login_session::log_out(&state.database, caller).await?;
    Ok(Envelope::builder().data(()).build())
}

impl LoginRequest {
    fn validate(&self) -> Result<(), ApiError> {
        validate_field("username", &self.username)?;
        validate_field("password", &self.password)
    }
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

pub(super) fn invalid_json(error: JsonRejection) -> ApiError {
    if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
        return ApiError::PayloadTooLarge;
    }
    ApiError::InvalidBody(vec![
        FieldError::builder()
            .field("body")
            .code("invalid")
            .message("Expected a JSON object with the required string fields.")
            .build(),
    ])
}
