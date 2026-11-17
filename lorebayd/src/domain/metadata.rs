use typed_builder::TypedBuilder;

pub(crate) const MCP_PATH: &str = "/mcp";
pub(crate) const AUTHORIZATION_PATH: &str = "/oauth/authorize";
pub(crate) const TOKEN_PATH: &str = "/oauth/token";
pub(crate) const REGISTRATION_PATH: &str = "/oauth/register";

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AuthorizationServer {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ProtectedResource {
    pub resource: String,
    pub authorization_server: String,
}

pub(crate) fn authorization_server_metadata(public_url: &str) -> AuthorizationServer {
    let issuer = public_url.trim_end_matches('/');
    AuthorizationServer::builder()
        .issuer(issuer)
        .authorization_endpoint(format!("{issuer}{AUTHORIZATION_PATH}"))
        .token_endpoint(format!("{issuer}{TOKEN_PATH}"))
        .registration_endpoint(format!("{issuer}{REGISTRATION_PATH}"))
        .build()
}

pub(crate) fn protected_resource_metadata(public_url: &str) -> ProtectedResource {
    let issuer = public_url.trim_end_matches('/');
    ProtectedResource::builder()
        .resource(format!("{issuer}{MCP_PATH}"))
        .authorization_server(issuer)
        .build()
}
