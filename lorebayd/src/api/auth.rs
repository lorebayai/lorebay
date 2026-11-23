use axum::extract::FromRequestParts;
use axum::http::{HeaderMap, header, request::Parts};

use crate::domain::access::verify_agent;
use crate::domain::caller::VerifiedCaller;
use crate::domain::person_sign_in;
use crate::domain::token::TokenKind;
use crate::server::ServerState;

use super::ApiError;

impl FromRequestParts<ServerState> for VerifiedCaller {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer(&parts.headers).ok_or(ApiError::Unauthorized)?;
        let claims = state
            .identity
            .signer
            .verify(token)
            .map_err(|_| ApiError::Unauthorized)?;
        match claims.kind {
            TokenKind::AgentSignIn => Ok(Self::Agent(verify_agent(&state.database, claims).await?)),
            TokenKind::PersonSignIn => Ok(Self::Person(
                person_sign_in::verify(&state.database, claims).await?,
            )),
        }
    }
}

pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .split_once(' ')
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
        .map(|(_, value)| value)
}
