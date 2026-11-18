use std::net::SocketAddr;

use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use lorebay_lib_types::Secret;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::api::{ApiError, Envelope, FieldError};
use crate::domain::caller::VerifiedCaller;
use crate::domain::device_sign_in::{
    self, DeviceDecision, DeviceDetails, DeviceError, DeviceVerification,
};
use crate::domain::login_session::LoginError;
use crate::oauth::no_store_headers;
use crate::server::ServerState;

use super::login_session::invalid_json;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ReadRequest {
    user_code: Secret<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct DecisionRequest {
    user_code: Secret<String>,
    decision: Decision,
}

impl From<DeviceError> for ApiError {
    fn from(error: DeviceError) -> Self {
        match error {
            DeviceError::Caller(error) => error.into(),
            DeviceError::InvalidUserCode => Self::InvalidBody(vec![
                FieldError::builder()
                    .field("user_code")
                    .code("invalid")
                    .message("Unknown, expired or already decided user code.")
                    .build(),
            ]),
            DeviceError::RateLimit(error) => LoginError::from(error).into(),
            error => {
                tracing::error!(%error, "device verification operation failed");
                Self::Internal
            }
        }
    }
}

pub(crate) async fn read(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    Extension(peer): Extension<SocketAddr>,
    body: Result<Json<ReadRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(input) = body.map_err(invalid_json)?;
    let details = device_sign_in::read_device_request(
        &state.identity,
        &state.database,
        &caller,
        DeviceVerification::builder()
            .user_code(input.user_code)
            .address(peer.ip())
            .build(),
    )
    .await?;
    let body = Envelope::<DeviceResponse>::builder().data(details).build();
    Ok((no_store_headers(), body).into_response())
}

pub(crate) async fn decide(
    State(state): State<ServerState>,
    caller: VerifiedCaller,
    Extension(peer): Extension<SocketAddr>,
    body: Result<Json<DecisionRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(input) = body.map_err(invalid_json)?;
    let decision = match input.decision {
        Decision::Approve => DeviceDecision::Approve,
        Decision::Decline => DeviceDecision::Decline,
    };
    device_sign_in::decide_device_request(
        &state.identity,
        &state.database,
        &caller,
        DeviceVerification::builder()
            .user_code(input.user_code)
            .address(peer.ip())
            .build(),
        decision,
    )
    .await?;
    Ok((
        no_store_headers(),
        Envelope::<()>::builder().data(()).build(),
    )
        .into_response())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Approve,
    Decline,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct DeviceResponse {
    user_code: Secret<String>,
    created_at: i64,
}

impl From<DeviceDetails> for DeviceResponse {
    fn from(details: DeviceDetails) -> Self {
        Self::builder()
            .user_code(details.user_code)
            .created_at(details.created_at)
            .build()
    }
}
