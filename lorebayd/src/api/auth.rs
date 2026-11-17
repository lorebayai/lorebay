use axum::extract::FromRequestParts;
use axum::http::{header, request::Parts};

use crate::domain::access::Caller;
use crate::domain::login_session;
use crate::server::ServerState;

use super::ApiError;

impl FromRequestParts<ServerState> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &ServerState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split_once(' '))
            .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("Bearer"))
            .map(|(_, token)| token)
            .ok_or(ApiError::Unauthorized)?;
        let claims = state
            .identity
            .signer
            .verify(token)
            .map_err(|_| ApiError::Unauthorized)?;
        login_session::verify(&state.database, claims)
            .await
            .map_err(ApiError::from)
    }
}
