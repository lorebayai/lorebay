use typed_builder::TypedBuilder;

pub(crate) const MCP_PATH: &str = "/mcp";
pub(crate) const AUTHORIZATION_PATH: &str = "/oauth/authorize";
pub(crate) const TOKEN_PATH: &str = "/oauth/token";
pub(crate) const DEVICE_AUTHORIZATION_PATH: &str = "/oauth/device";
pub(crate) const DEVICE_VERIFICATION_PATH: &str = "/oauth/device/verify";
pub(crate) const REGISTRATION_PATH: &str = "/oauth/register";

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AuthorizationServer {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: String,
    pub device_authorization_endpoint: String,
}

pub(crate) fn authorization_server_metadata(public_url: &str) -> AuthorizationServer {
    let issuer = public_url.trim_end_matches('/');
    AuthorizationServer::builder()
        .issuer(issuer)
        .authorization_endpoint(format!("{issuer}{AUTHORIZATION_PATH}"))
        .token_endpoint(format!("{issuer}{TOKEN_PATH}"))
        .registration_endpoint(format!("{issuer}{REGISTRATION_PATH}"))
        .device_authorization_endpoint(format!("{issuer}{DEVICE_AUTHORIZATION_PATH}"))
        .build()
}

pub(crate) fn mcp_resource(public_url: &str) -> String {
    format!("{}{MCP_PATH}", public_url.trim_end_matches('/'))
}
