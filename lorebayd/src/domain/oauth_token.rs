use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, Set, SqlErr,
    SqliteTransactionMode, TransactionOptions, TransactionTrait,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::account::Model as Account;
use crate::entity::agent_authorization::ActiveModel;
use crate::entity::oauth_code::Model as Code;
use crate::repository::account::AccountRepository;
use crate::repository::agent_authorization::AgentAuthorizationRepository;
use crate::repository::oauth_code::OauthCodeRepository;

use super::authorization::CLI_CLIENT_ID;
use super::login_session::{
    self, Identity, LoginError, Tokens, issue_tokens, new_refresh_token, now_millis, session_expiry,
};
use super::token::{TokenKind, TokenSubject, token_hash};
use super::{primary_key, random_id};

#[derive(Clone, Debug)]
pub(crate) enum TokenGrant {
    AuthorizationCode(CodeExchange),
    RefreshToken(TokenRefresh),
}

#[derive(Debug, Error)]
pub(crate) enum TokenError {
    #[error("Agent name is already taken")]
    AgentNameTaken,
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid grant")]
    InvalidGrant,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct CodeExchange {
    pub client_id: String,
    pub code: String,
    pub redirect_uri: String,
    pub code_verifier: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TokenRefresh {
    pub client_id: String,
    pub refresh_token: String,
}

impl From<LoginError> for TokenError {
    fn from(error: LoginError) -> Self {
        match error {
            LoginError::Database(error) => Self::Database(error),
            LoginError::Internal(error) => Self::Internal(error),
            LoginError::Unauthorized => Self::InvalidGrant,
            LoginError::RateLimited(_) => Self::Internal(anyhow::anyhow!(
                "unexpected rate limit during token issuance"
            )),
        }
    }
}

pub(crate) async fn grant_tokens(
    identity: &Identity,
    database: &DatabaseConnection,
    grant: TokenGrant,
) -> Result<Tokens, TokenError> {
    match grant {
        TokenGrant::AuthorizationCode(input) => exchange_code(identity, database, input).await,
        TokenGrant::RefreshToken(input) if input.client_id == CLI_CLIENT_ID => {
            Ok(login_session::refresh(identity, database, &input.refresh_token).await?)
        }
        TokenGrant::RefreshToken(input) => refresh_agent(identity, database, input).await,
    }
}

async fn exchange_code(
    identity: &Identity,
    database: &DatabaseConnection,
    input: CodeExchange,
) -> Result<Tokens, TokenError> {
    let transaction = begin_write(database).await?;
    let repository = OauthCodeRepository::builder()
        .connection(&transaction)
        .build();
    let now = now_millis()?;
    let code = repository
        .by_hash(&token_hash(&input.code))
        .await?
        .ok_or(TokenError::InvalidGrant)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(input.code_verifier.as_bytes()));
    if code.expires_at <= now
        || code.client_id != input.client_id
        || code.redirect_uri != input.redirect_uri
        || code.code_challenge != challenge
    {
        return Err(TokenError::InvalidGrant);
    }
    let account = active_account(&transaction, &code.account_id).await?;
    repository.delete(&code.id).await?;
    let tokens = if code.client_id == CLI_CLIENT_ID {
        login_session::create_session(identity, &transaction, account).await?
    } else {
        create_agent(identity, &transaction, code, account, now).await?
    };
    transaction.commit().await?;
    Ok(tokens)
}

async fn create_agent(
    identity: &Identity,
    transaction: &DatabaseTransaction,
    code: Code,
    account: Account,
    now: i64,
) -> Result<Tokens, TokenError> {
    let random_id = random_id().map_err(anyhow::Error::from)?;
    let refresh_token = new_refresh_token()?;
    let tokens = issue_tokens(
        identity,
        TokenSubject::builder()
            .username(account.username.as_str())
            .random_id(random_id.as_str())
            .kind(TokenKind::AgentAuthorization)
            .build(),
        refresh_token.clone(),
        now,
    )?;
    let model = ActiveModel {
        id: Set(primary_key()),
        account_id: Set(account.id),
        client_id: Set(code.client_id),
        name: Set(code.agent_name.ok_or(TokenError::InvalidGrant)?),
        random_id: Set(random_id),
        refresh_token_hash: Set(token_hash(&refresh_token)),
        refresh_expires_at: Set(session_expiry(identity, now)),
        created_at: Set(now),
    };
    AgentAuthorizationRepository::builder()
        .connection(transaction)
        .build()
        .insert(model)
        .await
        .map_err(|error| match error.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(_)) => TokenError::AgentNameTaken,
            _ => TokenError::Database(error),
        })?;
    Ok(tokens)
}

async fn refresh_agent(
    identity: &Identity,
    database: &DatabaseConnection,
    input: TokenRefresh,
) -> Result<Tokens, TokenError> {
    let transaction = begin_write(database).await?;
    let repository = AgentAuthorizationRepository::builder()
        .connection(&transaction)
        .build();
    let now = now_millis()?;
    let authorization = repository
        .by_refresh_hash(&token_hash(&input.refresh_token))
        .await?
        .filter(|row| row.client_id == input.client_id && row.refresh_expires_at > now)
        .ok_or(TokenError::InvalidGrant)?;
    let account = active_account(&transaction, &authorization.account_id).await?;
    let refresh_token = new_refresh_token()?;
    let tokens = issue_tokens(
        identity,
        TokenSubject::builder()
            .username(account.username.as_str())
            .random_id(authorization.random_id.as_str())
            .kind(TokenKind::AgentAuthorization)
            .build(),
        refresh_token.clone(),
        now,
    )?;
    let mut model: ActiveModel = authorization.into();
    model.refresh_token_hash = Set(token_hash(&refresh_token));
    model.refresh_expires_at = Set(session_expiry(identity, now));
    repository.update(model).await?;
    transaction.commit().await?;
    Ok(tokens)
}

async fn active_account<C: ConnectionTrait>(
    connection: &C,
    id: &str,
) -> Result<Account, TokenError> {
    AccountRepository::builder()
        .connection(connection)
        .build()
        .by_id(id)
        .await?
        .filter(|account| account.deleted_at.is_none())
        .ok_or(TokenError::InvalidGrant)
}

async fn begin_write(database: &DatabaseConnection) -> Result<DatabaseTransaction, TokenError> {
    Ok(database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await?)
}
