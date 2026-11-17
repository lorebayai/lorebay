use std::net::SocketAddr;

use axum::extract::State;
use axum::extract::rejection::FormRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Form, Json};
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::domain::DEVICE_VERIFICATION_PATH;
use crate::domain::device_sign_in::request_device_code;

use super::OAuthState;
use super::error::{OAuthError, no_store_headers};

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(super) struct DeviceRequest {
    client_id: String,
}

pub(super) async fn request(
    State(state): State<OAuthState>,
    Extension(peer): Extension<SocketAddr>,
    body: Result<Form<DeviceRequest>, FormRejection>,
) -> Result<Response, OAuthError> {
    let Form(input) = body.map_err(|error| {
        OAuthError::InvalidDeviceRequest(match error.status() {
            StatusCode::UNPROCESSABLE_ENTITY => StatusCode::BAD_REQUEST,
            status => status,
        })
    })?;
    if input.client_id.is_empty() {
        return Err(OAuthError::InvalidDeviceRequest(StatusCode::BAD_REQUEST));
    }
    let codes = request_device_code(
        &state.identity,
        &state.database,
        peer.ip(),
        &input.client_id,
    )
    .await?;
    let verification_uri = format!(
        "{}{DEVICE_VERIFICATION_PATH}",
        state.public_url.trim_end_matches('/')
    );
    let body = DeviceResponse::builder()
        .verification_uri_complete(format!("{verification_uri}?user_code={}", codes.user_code))
        .verification_uri(verification_uri)
        .device_code(codes.device_code)
        .user_code(codes.user_code)
        .expires_in(state.identity.config.device_request_lifetime_seconds)
        .interval(state.identity.config.device_poll_interval_seconds)
        .build();
    Ok((no_store_headers(), Json(body)).into_response())
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct DeviceResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: u64,
    interval: u64,
}
