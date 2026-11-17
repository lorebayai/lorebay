use sea_orm::{
    ConnectionTrait, DatabaseConnection, Set, SqliteTransactionMode, TransactionOptions,
    TransactionTrait,
};
use serde::Deserialize;
use thiserror::Error;
use typed_builder::TypedBuilder;
use url::{Host, Url};

use crate::entity::oauth_code::ActiveModel;
use crate::repository::OAuthClientRepository;
use crate::repository::agent_authorization::AgentAuthorizationRepository;
use crate::repository::oauth_code::OauthCodeRepository;

use super::access::Caller;
use super::caller::{CallerError, VerifiedCaller};
use super::login_session::now_millis;
use super::token::token_hash;
use super::{authorization_server_metadata, primary_key, random_id};

pub(crate) const CLI_CLIENT_ID: &str = "lorebay-cli";

#[derive(Clone, Debug)]
pub(crate) enum AgentNameConflict {
    Reject,
    Replace,
}

#[derive(Clone, Debug)]
pub(crate) enum AuthorizationDecision {
    Approve {
        agent_name: Option<String>,
        conflict: AgentNameConflict,
    },
    Decline,
}

#[derive(Debug, Error)]
pub(crate) enum AuthorizationError {
    #[error("Agent name is already taken")]
    AgentNameTaken,
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid authorization parameter")]
    InvalidParameter(&'static str),
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AuthorizationInput {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub state: Option<String>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct AuthorizationDetails {
    pub client_name: String,
    pub redirect_host: String,
    pub default_agent_name: Option<String>,
}

pub(crate) async fn read_authorization_request(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    input: &AuthorizationInput,
) -> Result<AuthorizationDetails, AuthorizationError> {
    caller.person()?;
    client_details(database, input).await
}

pub(crate) async fn decide_authorization(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    input: AuthorizationInput,
    decision: AuthorizationDecision,
    public_url: &str,
) -> Result<String, AuthorizationError> {
    let caller = caller.person()?;
    let transaction = database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await?;
    let details = client_details(&transaction, &input).await?;
    let response = match decision {
        AuthorizationDecision::Approve {
            agent_name,
            conflict,
        } => {
            let agent_name = chosen_agent_name(details, agent_name)?;
            let code = issue_code(&transaction, caller, &input, agent_name, conflict).await?;
            ("code", code)
        }
        AuthorizationDecision::Decline => ("error", "access_denied".to_owned()),
    };
    let redirect = redirect_url(&input, public_url, response)?;
    transaction.commit().await?;
    Ok(redirect)
}

const CODE_LIFETIME_MILLIS: i64 = 300_000;

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct RegisteredRedirects {
    redirect_uris: Vec<String>,
}

async fn issue_code<C: ConnectionTrait>(
    connection: &C,
    caller: &Caller,
    input: &AuthorizationInput,
    agent_name: Option<String>,
    conflict: AgentNameConflict,
) -> Result<String, AuthorizationError> {
    let existing = if let Some(name) = &agent_name {
        AgentAuthorizationRepository::builder()
            .connection(connection)
            .build()
            .by_account_and_name(&caller.account_id, name)
            .await?
    } else {
        None
    };
    let replaces_authorization_id = match (existing, conflict) {
        (Some(_), AgentNameConflict::Reject) => return Err(AuthorizationError::AgentNameTaken),
        (existing, _) => existing.map(|authorization| authorization.id),
    };
    let code = random_id().map_err(anyhow::Error::from)?;
    let now = now_millis()?;
    let model = ActiveModel {
        id: Set(primary_key()),
        code_hash: Set(token_hash(&code)),
        account_id: Set(caller.account_id.clone()),
        client_id: Set(input.client_id.clone()),
        redirect_uri: Set(input.redirect_uri.clone()),
        code_challenge: Set(input.code_challenge.clone()),
        agent_name: Set(agent_name),
        replaces_authorization_id: Set(replaces_authorization_id),
        created_at: Set(now),
        expires_at: Set(now + CODE_LIFETIME_MILLIS),
        exchanged_at: Set(None),
        agent_authorization_id: Set(None),
        login_session_id: Set(None),
    };
    OauthCodeRepository::builder()
        .connection(connection)
        .build()
        .insert(model)
        .await?;
    Ok(code)
}

async fn client_details<C: ConnectionTrait>(
    connection: &C,
    input: &AuthorizationInput,
) -> Result<AuthorizationDetails, AuthorizationError> {
    let redirect = Url::parse(&input.redirect_uri)
        .map_err(|_| AuthorizationError::InvalidParameter("redirect_uri"))?;
    let name = if input.client_id == CLI_CLIENT_ID {
        if !loopback_redirect(&redirect) {
            return Err(AuthorizationError::InvalidParameter("redirect_uri"));
        }
        None
    } else {
        let client = OAuthClientRepository::new(connection)
            .by_client_id(&input.client_id)
            .await?
            .ok_or(AuthorizationError::InvalidParameter("client_id"))?;
        let metadata: RegisteredRedirects =
            serde_json::from_str(&client.metadata).map_err(anyhow::Error::from)?;
        if !metadata.redirect_uris.contains(&input.redirect_uri) {
            return Err(AuthorizationError::InvalidParameter("redirect_uri"));
        }
        Some(client.client_name)
    };
    Ok(AuthorizationDetails::builder()
        .client_name(name.clone().unwrap_or_else(|| "lorebay CLI".to_owned()))
        .redirect_host(redirect.host_str().unwrap_or_default())
        .default_agent_name(name)
        .build())
}

fn loopback_redirect(url: &Url) -> bool {
    let loopback = match url.host() {
        Some(Host::Domain("localhost")) => true,
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        _ => false,
    };
    loopback
        && matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
}

fn chosen_agent_name(
    details: AuthorizationDetails,
    name: Option<String>,
) -> Result<Option<String>, AuthorizationError> {
    match (details.default_agent_name, name) {
        (None, None) => Ok(None),
        (Some(default), name) => {
            let name = name.unwrap_or(default);
            if name.trim().is_empty() || matches!(name.as_str(), "." | "..") {
                return Err(AuthorizationError::InvalidParameter("agent_name"));
            }
            Ok(Some(name))
        }
        (None, Some(_)) => Err(AuthorizationError::InvalidParameter("agent_name")),
    }
}

fn redirect_url(
    input: &AuthorizationInput,
    public_url: &str,
    response: (&str, String),
) -> Result<String, AuthorizationError> {
    let mut redirect = Url::parse(&input.redirect_uri)
        .map_err(|_| AuthorizationError::InvalidParameter("redirect_uri"))?;
    let query: Vec<_> = redirect
        .query_pairs()
        .filter(|(key, _)| !matches!(key.as_ref(), "code" | "error" | "state" | "iss"))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    redirect.set_query(None);
    let mut parameters = redirect.query_pairs_mut();
    parameters.extend_pairs(query);
    parameters.append_pair(response.0, &response.1);
    if let Some(state) = &input.state {
        parameters.append_pair("state", state);
    }
    parameters.append_pair("iss", &authorization_server_metadata(public_url).issuer);
    drop(parameters);
    Ok(redirect.into())
}
