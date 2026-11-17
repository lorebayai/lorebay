use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use url::Url;

use crate::api::{ApiError, Envelope, FieldError};
use crate::domain::access::Caller;
use crate::domain::authorization::{
    self, AuthorizationDecision, AuthorizationDetails, AuthorizationError, AuthorizationInput,
};
use crate::domain::{authorization_server_metadata, protected_resource_metadata};
use crate::oauth::no_store_headers;
use crate::server::ServerState;

use super::login_session::invalid_json;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AuthorizationRequest {
    client_id: String,
    redirect_uri: String,
    response_type: String,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    state: Option<String>,
    resource: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct DecisionRequest {
    #[serde(flatten)]
    authorization: AuthorizationRequest,
    decision: Decision,
    agent_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Approve,
    Decline,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct AuthorizationResponse {
    client_name: String,
    redirect_host: String,
    default_agent_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct DecisionResponse {
    redirect_url: String,
}

impl AuthorizationRequest {
    fn validate(self, public_url: &str) -> Result<AuthorizationInput, ApiError> {
        if self.client_id.is_empty() {
            return Err(invalid_parameter("client_id"));
        }
        if Url::parse(&self.redirect_uri).is_err() || self.redirect_uri.trim() != self.redirect_uri
        {
            return Err(invalid_parameter("redirect_uri"));
        }
        if self.response_type != "code" {
            return Err(invalid_parameter("response_type"));
        }
        let challenge = self
            .code_challenge
            .ok_or_else(|| invalid_parameter("code_challenge"))?;
        if URL_SAFE_NO_PAD
            .decode(&challenge)
            .map_or(true, |bytes| bytes.len() != 32)
        {
            return Err(invalid_parameter("code_challenge"));
        }
        if self.code_challenge_method.as_deref() != Some("S256") {
            return Err(invalid_parameter("code_challenge_method"));
        }
        if self.resource.as_deref().is_some_and(|resource| {
            resource != authorization_server_metadata(public_url).issuer
                && resource != protected_resource_metadata(public_url).resource
        }) {
            return Err(invalid_parameter("resource"));
        }
        Ok(AuthorizationInput::builder()
            .client_id(self.client_id)
            .redirect_uri(self.redirect_uri)
            .code_challenge(challenge)
            .state(self.state)
            .build())
    }
}

impl From<AuthorizationDetails> for AuthorizationResponse {
    fn from(details: AuthorizationDetails) -> Self {
        Self::builder()
            .client_name(details.client_name)
            .redirect_host(details.redirect_host)
            .default_agent_name(details.default_agent_name)
            .build()
    }
}

impl From<AuthorizationError> for ApiError {
    fn from(error: AuthorizationError) -> Self {
        match error {
            AuthorizationError::InvalidParameter(field) => invalid_parameter(field),
            error => {
                tracing::error!(%error, "authorization operation failed");
                Self::Internal
            }
        }
    }
}

pub(crate) async fn read(
    State(state): State<ServerState>,
    caller: Caller,
    body: Result<Json<AuthorizationRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = body.map_err(invalid_json)?;
    let input = request.validate(&state.identity.config.public_url)?;
    let details =
        authorization::read_authorization_request(&state.database, &caller, &input).await?;
    let body = Envelope::<AuthorizationResponse>::builder()
        .data(details)
        .build();
    Ok((no_store_headers(), body).into_response())
}

pub(crate) async fn decide(
    State(state): State<ServerState>,
    caller: Caller,
    body: Result<Json<DecisionRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = body.map_err(invalid_json)?;
    let public_url = &state.identity.config.public_url;
    let input = request.authorization.validate(public_url)?;
    let decision = match request.decision {
        Decision::Approve => AuthorizationDecision::Approve(request.agent_name),
        Decision::Decline => AuthorizationDecision::Decline,
    };
    let redirect =
        authorization::decide_authorization(&state.database, &caller, input, decision, public_url)
            .await?;
    let body = Envelope::<DecisionResponse>::builder()
        .data(DecisionResponse::builder().redirect_url(redirect).build())
        .build();
    Ok((no_store_headers(), body).into_response())
}

fn invalid_parameter(field: &str) -> ApiError {
    ApiError::InvalidBody(vec![
        FieldError::builder()
            .field(field)
            .code("invalid")
            .message("Invalid authorization parameter.")
            .build(),
    ])
}
