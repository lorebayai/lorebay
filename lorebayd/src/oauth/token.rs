use axum::extract::State;
use axum::extract::rejection::FormRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Form, Json};
use lorebay_lib_types::Secret;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;

use crate::domain::login_session::Tokens;
use crate::domain::oauth_token::{CodeExchange, TokenGrant, TokenRefresh, grant_tokens};
use crate::domain::{authorization_server_metadata, protected_resource_metadata};

use super::error::{OAuthError, no_store_headers};
use super::{AUTHORIZATION_CODE_GRANT, OAuthState, REFRESH_TOKEN_GRANT};

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(super) struct TokenRequest {
    grant_type: String,
    client_id: String,
    code: Option<Secret<String>>,
    redirect_uri: Option<String>,
    code_verifier: Option<Secret<String>>,
    refresh_token: Option<Secret<String>>,
    resource: Option<String>,
    device_code: Option<Secret<String>>,
}

impl TokenRequest {
    fn validate(self, public_url: &str) -> Result<TokenGrant, OAuthError> {
        if self.resource.as_deref().is_some_and(|resource| {
            resource != authorization_server_metadata(public_url).issuer
                && resource != protected_resource_metadata(public_url).resource
        }) {
            return Err(OAuthError::InvalidTarget);
        }
        let client_id = required(Some(self.client_id))?;
        match self.grant_type.as_str() {
            AUTHORIZATION_CODE_GRANT => {
                let code_verifier = required_secret(self.code_verifier)?;
                if !(43..=128).contains(&code_verifier.expose().len())
                    || !code_verifier.expose().bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
                    })
                {
                    return Err(OAuthError::InvalidTokenRequest(StatusCode::BAD_REQUEST));
                }
                Ok(TokenGrant::AuthorizationCode(
                    CodeExchange::builder()
                        .client_id(client_id)
                        .code(required_secret(self.code)?)
                        .redirect_uri(required(self.redirect_uri)?)
                        .code_verifier(code_verifier)
                        .build(),
                ))
            }
            super::DEVICE_CODE_GRANT => Ok(TokenGrant::DeviceCode {
                client_id,
                device_code: required_secret(self.device_code)?,
            }),
            REFRESH_TOKEN_GRANT => Ok(TokenGrant::RefreshToken(
                TokenRefresh::builder()
                    .client_id(client_id)
                    .refresh_token(required_secret(self.refresh_token)?)
                    .build(),
            )),
            "" => Err(OAuthError::InvalidTokenRequest(StatusCode::BAD_REQUEST)),
            _ => Err(OAuthError::UnsupportedGrantType),
        }
    }
}

pub(super) async fn token(
    State(state): State<OAuthState>,
    body: Result<Form<TokenRequest>, FormRejection>,
) -> Result<Response, OAuthError> {
    let Form(request) = body.map_err(|error| {
        let status = match error.status() {
            StatusCode::PAYLOAD_TOO_LARGE => StatusCode::PAYLOAD_TOO_LARGE,
            StatusCode::UNSUPPORTED_MEDIA_TYPE => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            _ => StatusCode::BAD_REQUEST,
        };
        OAuthError::InvalidTokenRequest(status)
    })?;
    let grant = request.validate(&state.public_url)?;
    let tokens = grant_tokens(&state.identity, &state.database, grant).await?;
    Ok((no_store_headers(), Json(TokenResponse::from(tokens))).into_response())
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct TokenResponse {
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

fn required(value: Option<String>) -> Result<String, OAuthError> {
    value
        .filter(|value| !value.is_empty())
        .ok_or(OAuthError::InvalidTokenRequest(StatusCode::BAD_REQUEST))
}

fn required_secret(value: Option<Secret<String>>) -> Result<Secret<String>, OAuthError> {
    value
        .filter(|value| !value.expose().is_empty())
        .ok_or(OAuthError::InvalidTokenRequest(StatusCode::BAD_REQUEST))
}
