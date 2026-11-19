use std::sync::Arc;

use axum::http::HeaderMap;
use futures_util::future::BoxFuture;
use lorebay_lib_core::{Caller, CallerResolver};
use sea_orm::DatabaseConnection;

use crate::api::bearer;
use crate::domain::access::{AccessError, verify_agent};
use crate::domain::caller::VerifiedCaller;
use crate::domain::login_session::Identity;

pub(super) struct AgentResolver {
    pub database: DatabaseConnection,
    pub identity: Arc<Identity>,
}

impl CallerResolver for AgentResolver {
    fn resolve<'a>(
        &'a self,
        headers: &'a HeaderMap,
    ) -> BoxFuture<'a, anyhow::Result<Option<Caller>>> {
        Box::pin(async move {
            let Some(value) = bearer(headers) else {
                return Ok(None);
            };
            let Ok(claims) = self.identity.signer.verify(value) else {
                return Ok(None);
            };
            match verify_agent(&self.database, claims).await {
                Ok(caller) => {
                    tracing::debug!(account_id = %caller.account_id, agent_authorization_id = %caller.authorization_id,
                        "authenticated MCP caller");
                    Ok(Some(VerifiedCaller::Agent(caller).library_caller()))
                }
                Err(AccessError::Unauthorized) => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
    }
}
