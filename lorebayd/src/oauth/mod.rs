mod error;
mod metadata;
mod register;

use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use lorebay_base::{ConfigError, RateLimiter, SystemClock};
use sea_orm::DatabaseConnection;
use typed_builder::TypedBuilder;

use crate::Config;
use crate::domain::REGISTRATION_PATH;

use error::OAuthError;

const AUTHORIZATION_METADATA_PATH: &str = "/.well-known/oauth-authorization-server";
const PROTECTED_RESOURCE_METADATA_PATH: &str = "/.well-known/oauth-protected-resource/mcp";
const AUTHORIZATION_CODE_GRANT: &str = "authorization_code";
const REFRESH_TOKEN_GRANT: &str = "refresh_token";
const CODE_RESPONSE_TYPE: &str = "code";
const PUBLIC_CLIENT_AUTH_METHOD: &str = "none";
const PKCE_METHOD: &str = "S256";

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct OAuthState {
    database: DatabaseConnection,
    public_url: String,
    registration_limiter: RateLimiter<IpAddr, SystemClock>,
    registration_limits: RegistrationLimits,
    clock: Arc<SystemClock>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct RegistrationLimits {
    client_name_max_bytes: usize,
    redirect_uri_limit: usize,
    redirect_uri_max_bytes: usize,
    body_limit_bytes: usize,
}

impl From<&Config> for RegistrationLimits {
    fn from(config: &Config) -> Self {
        Self::builder()
            .client_name_max_bytes(config.registration_client_name_max_bytes)
            .redirect_uri_limit(config.registration_redirect_uri_limit)
            .redirect_uri_max_bytes(config.registration_redirect_uri_max_bytes)
            .body_limit_bytes(config.registration_body_limit_bytes)
            .build()
    }
}

impl OAuthState {
    pub(crate) fn new(config: &Config, database: DatabaseConnection) -> Result<Self, ConfigError> {
        let limit = NonZeroU32::new(config.registration_rate_limit)
            .ok_or_else(|| ConfigError::invalid("registration_rate_limit", "must be positive"))?;
        let clock = Arc::new(SystemClock);
        Ok(Self::builder()
            .database(database)
            .public_url(config.public_url.clone())
            .registration_limits(RegistrationLimits::from(config))
            .registration_limiter(RateLimiter::new(
                limit,
                Duration::from_secs(config.registration_rate_window_seconds),
                clock.clone(),
            ))
            .clock(clock)
            .build())
    }
}

pub(crate) fn router(state: OAuthState) -> Router {
    Router::new()
        .route(
            AUTHORIZATION_METADATA_PATH,
            get(metadata::authorization_server),
        )
        .route(
            PROTECTED_RESOURCE_METADATA_PATH,
            get(metadata::protected_resource),
        )
        .route(
            REGISTRATION_PATH,
            post(register::register).layer(DefaultBodyLimit::max(
                state.registration_limits.body_limit_bytes,
            )),
        )
        .method_not_allowed_fallback(|| async { OAuthError::MethodNotAllowed })
        .with_state(state)
}
