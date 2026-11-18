use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use lorebay_base::{Clock, RateLimitError, RateLimiter, SystemClock};
use lorebay_lib_types::{Secret, sha256_hex, unix_millis};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, Set, SqliteTransactionMode,
    TransactionOptions, TransactionTrait,
};
use thiserror::Error;
use tokio::task::spawn_blocking;
use typed_builder::TypedBuilder;

use crate::Config;
use crate::entity::account::Model as Account;
use crate::entity::login_session::{ActiveModel, Model};
use crate::entity::refresh_token_history;
use crate::repository::account::AccountRepository;
use crate::repository::agent_authorization::AgentAuthorizationRepository;
use crate::repository::login_session::LoginSessionRepository;
use crate::repository::refresh_token_history::RefreshTokenHistoryRepository;

use super::access::Caller;
use super::caller::{CallerError, VerifiedCaller};
use super::token::{Claims, TokenKind, TokenSigner, TokenSubject};
use super::user_store::UserStore;
use super::{primary_key, random_id};

#[derive(Debug, Error)]
pub(crate) enum LoginError {
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Too Many Requests")]
    RateLimited(u64),
    #[error("Unauthorized")]
    Unauthorized,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Identity {
    pub user_store: Arc<dyn UserStore>,
    pub signer: TokenSigner,
    pub config: Config,
    pub limiter: RateLimiter<IpAddr, SystemClock>,
    pub device_limiter: RateLimiter<IpAddr, SystemClock>,
    pub device_verification_limiter: RateLimiter<IpAddr, SystemClock>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Tokens {
    pub access_token: Secret<String>,
    pub refresh_token: Secret<String>,
    pub expires_in: u64,
}

impl From<RateLimitError> for LoginError {
    fn from(error: RateLimitError) -> Self {
        match error {
            RateLimitError::Exceeded { retry_after } => Self::RateLimited(
                retry_after
                    .as_secs()
                    .saturating_add(u64::from(retry_after.subsec_nanos() != 0))
                    .max(1),
            ),
            RateLimitError::Unavailable => {
                Self::Internal(anyhow!("sign-in rate limiter is unavailable"))
            }
        }
    }
}

pub(crate) async fn log_in(
    identity: &Identity,
    database: &DatabaseConnection,
    address: IpAddr,
    username: String,
    password: Secret<String>,
) -> Result<Tokens, LoginError> {
    identity.limiter.check(address)?;
    let store = Arc::clone(&identity.user_store);
    let supplied_username = username.clone();
    let matches = spawn_blocking(move || store.verify(&supplied_username, password.expose()))
        .await
        .map_err(|error| anyhow!(error))??;
    if !matches {
        return Err(LoginError::Unauthorized);
    }
    let transaction = begin_write(database).await?;
    let account = AccountRepository::builder()
        .connection(&transaction)
        .build()
        .by_username(&username)
        .await?
        .filter(|account| account.deleted_at.is_none())
        .ok_or(LoginError::Unauthorized)?;
    let (_, tokens) = create_session(identity, &transaction, account).await?;
    transaction.commit().await?;
    Ok(tokens)
}

pub(crate) async fn refresh(
    identity: &Identity,
    database: &DatabaseConnection,
    token: &str,
) -> Result<Tokens, LoginError> {
    let transaction = begin_write(database).await?;
    let hash = Secret::from(sha256_hex(token));
    let history = RefreshTokenHistoryRepository::builder()
        .connection(&transaction)
        .build()
        .by_hash(hash.expose(), now_millis()?)
        .await?;
    if let Some(history) = history {
        if let Some(id) = history.agent_authorization_id {
            AgentAuthorizationRepository::builder()
                .connection(&transaction)
                .build()
                .delete_by_id(&id)
                .await?;
        }
        if let Some(id) = history.login_session_id {
            LoginSessionRepository::builder()
                .connection(&transaction)
                .build()
                .delete_by_id(&id)
                .await?;
        }
        transaction.commit().await?;
        return Err(LoginError::Unauthorized);
    }
    let tokens = rotate_refresh(identity, &transaction, &hash).await?;
    transaction.commit().await?;
    Ok(tokens)
}

pub(crate) async fn rotate_refresh(
    identity: &Identity,
    transaction: &DatabaseTransaction,
    hash: &Secret<String>,
) -> Result<Tokens, LoginError> {
    let repository = LoginSessionRepository::builder()
        .connection(transaction)
        .build();
    let session = repository
        .by_refresh_hash(hash.expose())
        .await?
        .ok_or(LoginError::Unauthorized)?;
    let now = now_millis()?;
    let username = active_username(transaction, &session, now).await?;
    let refresh_token = new_refresh_token()?;
    let tokens = issue_tokens(
        identity,
        TokenSubject::builder()
            .username(username.as_str())
            .random_id(session.random_id.as_str())
            .kind(TokenKind::LoginSession)
            .build(),
        refresh_token.clone(),
        now,
    )?;
    RefreshTokenHistoryRepository::builder()
        .connection(transaction)
        .build()
        .insert(refresh_token_history::ActiveModel {
            id: Set(primary_key()),
            token_hash: Set(hash.expose().clone()),
            agent_authorization_id: Set(None),
            login_session_id: Set(Some(session.id.clone())),
            expires_at: Set(session.expires_at),
        })
        .await?;
    let mut model: ActiveModel = session.into();
    model.refresh_token_hash = Set(sha256_hex(refresh_token.expose()));
    model.expires_at = Set(session_expiry(identity, now));
    repository.update(model).await?;
    Ok(tokens)
}

pub(crate) async fn log_out(
    database: &DatabaseConnection,
    caller: VerifiedCaller,
) -> Result<(), LoginError> {
    let caller = caller.person()?;
    let transaction = begin_write(database).await?;
    let repository = LoginSessionRepository::builder()
        .connection(&transaction)
        .build();
    if repository
        .delete(&caller.login_session_id, &caller.account_id)
        .await?
        == 0
    {
        return Err(LoginError::Unauthorized);
    }
    transaction.commit().await?;
    Ok(())
}

pub(crate) async fn verify(
    database: &DatabaseConnection,
    claims: Claims,
) -> Result<Caller, LoginError> {
    let now = now_millis()?;
    if claims.kind != TokenKind::LoginSession || claims.exp <= (now / 1000) as u64 {
        return Err(LoginError::Unauthorized);
    }
    let transaction = database.begin().await?;
    let session = LoginSessionRepository::builder()
        .connection(&transaction)
        .build()
        .by_random_id(&claims.sid)
        .await?
        .ok_or(LoginError::Unauthorized)?;
    let username = active_username(&transaction, &session, now).await?;
    if username != claims.sub {
        return Err(LoginError::Unauthorized);
    }
    transaction.commit().await?;
    Ok(Caller::builder()
        .account_id(session.account_id)
        .login_session_id(session.id)
        .build())
}

pub(crate) fn now_millis() -> anyhow::Result<i64> {
    Ok(unix_millis(SystemClock.now())?)
}

pub(crate) async fn create_session(
    identity: &Identity,
    transaction: &DatabaseTransaction,
    account: Account,
) -> Result<(String, Tokens), LoginError> {
    let now = now_millis()?;
    let random_id = random_id().map_err(anyhow::Error::from)?;
    let refresh_token = new_refresh_token()?;
    let tokens = issue_tokens(
        identity,
        TokenSubject::builder()
            .username(account.username.as_str())
            .random_id(random_id.as_str())
            .kind(TokenKind::LoginSession)
            .build(),
        refresh_token.clone(),
        now,
    )?;
    let id = primary_key();
    let model = ActiveModel {
        id: Set(id.clone()),
        account_id: Set(account.id),
        random_id: Set(random_id),
        refresh_token_hash: Set(sha256_hex(refresh_token.expose())),
        created_at: Set(now),
        expires_at: Set(session_expiry(identity, now)),
    };
    LoginSessionRepository::builder()
        .connection(transaction)
        .build()
        .insert(model)
        .await?;
    Ok((id, tokens))
}

pub(crate) fn issue_tokens(
    identity: &Identity,
    subject: TokenSubject<'_>,
    refresh_token: Secret<String>,
    now: i64,
) -> Result<Tokens, LoginError> {
    let now = (now / 1000) as u64;
    let expires_in = identity.config.access_token_lifetime_seconds;
    Ok(Tokens::builder()
        .access_token(identity.signer.sign(subject, now + expires_in, now)?)
        .refresh_token(refresh_token)
        .expires_in(expires_in)
        .build())
}

pub(crate) fn new_refresh_token() -> anyhow::Result<Secret<String>> {
    Ok(Secret::from(format!("{}{}", random_id()?, random_id()?)))
}

pub(crate) fn session_expiry(identity: &Identity, now: i64) -> i64 {
    now + Duration::from_secs(identity.config.refresh_token_lifetime_seconds).as_millis() as i64
}

async fn begin_write(database: &DatabaseConnection) -> Result<DatabaseTransaction, LoginError> {
    Ok(database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await?)
}

async fn active_username<C: ConnectionTrait>(
    connection: &C,
    session: &Model,
    now: i64,
) -> Result<String, LoginError> {
    if session.expires_at <= now {
        return Err(LoginError::Unauthorized);
    }
    let account = AccountRepository::builder()
        .connection(connection)
        .build()
        .by_id(&session.account_id)
        .await?
        .filter(|account| account.deleted_at.is_none())
        .ok_or(LoginError::Unauthorized)?;
    Ok(account.username)
}
