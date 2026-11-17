use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequestParts, State};
use axum::http::{StatusCode, request::Parts};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
use url::{Host, Url};

use crate::domain::{self, NewOAuthClient};
use crate::oauth::error::{OAuthError, no_store_headers};
use crate::oauth::{
    AUTHORIZATION_CODE_GRANT, CODE_RESPONSE_TYPE, OAuthState, PUBLIC_CLIENT_AUTH_METHOD,
    REFRESH_TOKEN_GRANT, RegistrationLimits,
};

#[derive(Clone, Debug, Deserialize, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(super) struct RegistrationRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    application_type: Option<String>,
    client_name: String,
    #[serde(default = "default_grant_types")]
    grant_types: Vec<String>,
    redirect_uris: Vec<String>,
    #[serde(default = "default_response_types")]
    response_types: Vec<String>,
    token_endpoint_auth_method: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct RegistrationResponse {
    client_id: String,
    #[serde(flatten)]
    metadata: RegistrationRequest,
}

pub(super) struct RegistrationAttempt;

impl FromRequestParts<OAuthState> for RegistrationAttempt {
    type Rejection = OAuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &OAuthState,
    ) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<SocketAddr>()
            .ok_or(OAuthError::Internal)?;
        state.registration_limiter.check(peer.ip())?;
        Ok(Self)
    }
}

impl TryFrom<&RegistrationRequest> for NewOAuthClient {
    type Error = OAuthError;

    fn try_from(request: &RegistrationRequest) -> Result<Self, Self::Error> {
        let metadata = serde_json::to_string(request).map_err(|error| {
            tracing::error!(%error, "could not encode OAuth client metadata");
            OAuthError::Internal
        })?;
        Ok(Self::builder()
            .client_name(request.client_name.clone())
            .metadata(metadata)
            .build())
    }
}

impl RegistrationRequest {
    fn validate(&self, limits: &RegistrationLimits) -> Result<(), OAuthError> {
        if self.client_name.len() > limits.client_name_max_bytes
            || self.redirect_uris.len() > limits.redirect_uri_limit
        {
            return Err(OAuthError::InvalidClientMetadata);
        }
        if self.redirect_uris.is_empty()
            || self
                .redirect_uris
                .iter()
                .any(|uri| uri.len() > limits.redirect_uri_max_bytes || !valid_redirect_uri(uri))
        {
            return Err(OAuthError::InvalidRedirectUri);
        }
        if self.token_endpoint_auth_method != PUBLIC_CLIENT_AUTH_METHOD
            || self.client_name.trim().is_empty()
            || self
                .application_type
                .as_deref()
                .is_some_and(|value| !matches!(value, "native" | "web"))
            || self.response_types != [CODE_RESPONSE_TYPE]
            || !self
                .grant_types
                .iter()
                .any(|grant| grant == AUTHORIZATION_CODE_GRANT)
            || self.grant_types.iter().any(|grant| {
                !matches!(
                    grant.as_str(),
                    AUTHORIZATION_CODE_GRANT | REFRESH_TOKEN_GRANT
                )
            })
        {
            return Err(OAuthError::InvalidClientMetadata);
        }
        Ok(())
    }
}

pub(super) async fn register(
    State(state): State<OAuthState>,
    _attempt: RegistrationAttempt,
    body: Result<Json<RegistrationRequest>, JsonRejection>,
) -> Result<Response, OAuthError> {
    let Json(metadata) = body.map_err(json_error)?;
    metadata.validate(&state.registration_limits)?;
    let input = NewOAuthClient::try_from(&metadata)?;
    let client_id = domain::register_client(&state.database, input, state.clock.as_ref()).await?;
    let body = RegistrationResponse::builder()
        .client_id(client_id)
        .metadata(metadata)
        .build();
    Ok((StatusCode::CREATED, no_store_headers(), Json(body)).into_response())
}

fn valid_redirect_uri(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let loopback = match url.host() {
        Some(Host::Domain("localhost")) => true,
        Some(Host::Ipv4(address)) => address == Ipv4Addr::LOCALHOST,
        Some(Host::Ipv6(address)) => address == Ipv6Addr::LOCALHOST,
        _ => false,
    };
    (url.scheme() == "https" || url.scheme() == "http" && loopback)
        && url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && value.trim() == value
}

fn json_error(error: JsonRejection) -> OAuthError {
    let status = match error.status() {
        StatusCode::PAYLOAD_TOO_LARGE => StatusCode::PAYLOAD_TOO_LARGE,
        StatusCode::UNSUPPORTED_MEDIA_TYPE => StatusCode::UNSUPPORTED_MEDIA_TYPE,
        _ => StatusCode::BAD_REQUEST,
    };
    OAuthError::InvalidRequest(status)
}

fn default_grant_types() -> Vec<String> {
    vec![AUTHORIZATION_CODE_GRANT.to_owned()]
}

fn default_response_types() -> Vec<String> {
    vec![CODE_RESPONSE_TYPE.to_owned()]
}
