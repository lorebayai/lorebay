use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::domain::access::{bearer, verify_agent};
use crate::domain::caller::VerifiedCaller;
use crate::domain::login_session;
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
            TokenKind::AgentAuthorization => {
                Ok(Self::Agent(verify_agent(&state.database, claims).await?))
            }
            TokenKind::LoginSession => Ok(Self::Person(
                login_session::verify(&state.database, claims).await?,
            )),
        }
    }
}
