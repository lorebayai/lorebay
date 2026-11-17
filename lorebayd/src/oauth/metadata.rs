use axum::Json;
use axum::extract::State;
use serde::Serialize;
use typed_builder::TypedBuilder;

use crate::domain::{self, AuthorizationServer, ProtectedResource};
use crate::oauth::{
    AUTHORIZATION_CODE_GRANT, CODE_RESPONSE_TYPE, OAuthState, PKCE_METHOD,
    PUBLIC_CLIENT_AUTH_METHOD, REFRESH_TOKEN_GRANT,
};

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(super) struct AuthorizationServerResponse {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: String,
    response_types_supported: [&'static str; 1],
    grant_types_supported: [&'static str; 2],
    code_challenge_methods_supported: [&'static str; 1],
    token_endpoint_auth_methods_supported: [&'static str; 1],
    authorization_response_iss_parameter_supported: bool,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(super) struct ProtectedResourceResponse {
    resource: String,
    authorization_servers: [String; 1],
}

impl From<AuthorizationServer> for AuthorizationServerResponse {
    fn from(metadata: AuthorizationServer) -> Self {
        Self::builder()
            .issuer(metadata.issuer)
            .authorization_endpoint(metadata.authorization_endpoint)
            .token_endpoint(metadata.token_endpoint)
            .registration_endpoint(metadata.registration_endpoint)
            .response_types_supported([CODE_RESPONSE_TYPE])
            .grant_types_supported([AUTHORIZATION_CODE_GRANT, REFRESH_TOKEN_GRANT])
            .code_challenge_methods_supported([PKCE_METHOD])
            .token_endpoint_auth_methods_supported([PUBLIC_CLIENT_AUTH_METHOD])
            .authorization_response_iss_parameter_supported(true)
            .build()
    }
}

impl From<ProtectedResource> for ProtectedResourceResponse {
    fn from(metadata: ProtectedResource) -> Self {
        Self::builder()
            .resource(metadata.resource)
            .authorization_servers([metadata.authorization_server])
            .build()
    }
}

pub(super) async fn authorization_server(
    State(state): State<OAuthState>,
) -> Json<AuthorizationServerResponse> {
    Json(domain::authorization_server_metadata(&state.public_url).into())
}

pub(super) async fn protected_resource(
    State(state): State<OAuthState>,
) -> Json<ProtectedResourceResponse> {
    Json(domain::protected_resource_metadata(&state.public_url).into())
}
