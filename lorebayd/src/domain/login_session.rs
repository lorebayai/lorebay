use std::net::IpAddr;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::anyhow;
use lorebay_base::{Clock, RateLimiter, SystemClock};
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
use crate::repository::account::AccountRepository;
use crate::repository::login_session::LoginSessionRepository;

use super::access::Caller;
use super::id::{primary_key, random_id};
use super::password::verify_password;
use super::token::{Claims, TokenKind, TokenSigner, token_hash};
use super::users::Users;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Identity {
    pub users: Users,
    pub signer: TokenSigner,
    pub config: Config,
    pub limiter: RateLimiter<IpAddr, SystemClock>,
}

#[derive(Debug, Error)]
pub(crate) enum LoginError {
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
pub(crate) struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

pub(crate) async fn log_in(
    identity: &Identity,
    database: &DatabaseConnection,
    address: IpAddr,
    username: String,
    password: String,
) -> Result<Tokens, LoginError> {
    identity
        .limiter
        .check(address)
        .map_err(|wait| LoginError::RateLimited(wait.as_secs().saturating_add(1)))?;
    let hash = identity
        .users
        .hashes
        .get(&username)
        .unwrap_or(&identity.users.dummy_hash)
        .clone();
    let matches = spawn_blocking(move || verify_password(&password, &hash))
        .await
        .map_err(|error| anyhow!(error))?;
    if !matches || !identity.users.hashes.contains_key(&username) {
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
    let tokens = create_session(identity, &transaction, account).await?;
    transaction.commit().await?;
    Ok(tokens)
}

pub(crate) async fn refresh(
    identity: &Identity,
    database: &DatabaseConnection,
    token: &str,
) -> Result<Tokens, LoginError> {
    let transaction = begin_write(database).await?;
    let repository = LoginSessionRepository::builder()
        .connection(&transaction)
        .build();
    let session = repository
        .by_refresh_hash(&token_hash(token))
        .await?
        .ok_or(LoginError::Unauthorized)?;
    let now = now_millis()?;
    let username = active_username(&transaction, &session, now).await?;
    let refresh_token = new_refresh_token()?;
    let tokens = issue_tokens(
        identity,
        &username,
        &session.random_id,
        refresh_token.clone(),
        now,
    )?;
    let mut model: ActiveModel = session.into();
    model.refresh_token_hash = Set(token_hash(&refresh_token));
    model.expires_at = Set(session_expiry(identity, now));
    repository.update(model).await?;
    transaction.commit().await?;
    Ok(tokens)
}

pub(crate) async fn log_out(
    database: &DatabaseConnection,
    caller: Caller,
) -> Result<(), LoginError> {
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
    Ok(i64::try_from(
        SystemClock.now().duration_since(UNIX_EPOCH)?.as_millis(),
    )?)
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

async fn create_session(
    identity: &Identity,
    transaction: &DatabaseTransaction,
    account: Account,
) -> Result<Tokens, LoginError> {
    let now = now_millis()?;
    let random_id = random_id()?;
    let refresh_token = new_refresh_token()?;
    let tokens = issue_tokens(
        identity,
        &account.username,
        &random_id,
        refresh_token.clone(),
        now,
    )?;
    let model = ActiveModel {
        id: Set(primary_key()),
        account_id: Set(account.id),
        random_id: Set(random_id),
        refresh_token_hash: Set(token_hash(&refresh_token)),
        created_at: Set(now),
        expires_at: Set(session_expiry(identity, now)),
    };
    LoginSessionRepository::builder()
        .connection(transaction)
        .build()
        .insert(model)
        .await?;
    Ok(tokens)
}

fn issue_tokens(
    identity: &Identity,
    username: &str,
    session: &str,
    refresh_token: String,
    now: i64,
) -> Result<Tokens, LoginError> {
    let now = (now / 1000) as u64;
    let expires_in = identity.config.access_token_lifetime_seconds;
    Ok(Tokens::builder()
        .access_token(
            identity
                .signer
                .sign(username, session, now + expires_in, now)?,
        )
        .refresh_token(refresh_token)
        .expires_in(expires_in)
        .build())
}

fn new_refresh_token() -> anyhow::Result<String> {
    Ok(format!("{}{}", random_id()?, random_id()?))
}

fn session_expiry(identity: &Identity, now: i64) -> i64 {
    now + Duration::from_secs(identity.config.refresh_token_lifetime_seconds).as_millis() as i64
}
