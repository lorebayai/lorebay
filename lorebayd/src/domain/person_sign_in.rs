use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use lorebay_base::{RateLimitError, RateLimiter, SystemClock};
use lorebay_lib_types::{Secret, sha256_hex};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, Set, SqliteTransactionMode,
    TransactionOptions, TransactionTrait,
};
use thiserror::Error;
use tokio::task::spawn_blocking;
use typed_builder::TypedBuilder;

use crate::Config;
use crate::entity::account::Model as Account;
use crate::entity::person_sign_in::{ActiveModel, Model};
use crate::entity::refresh_token_history;
use crate::repository::account::AccountRepository;
use crate::repository::agent_sign_in::AgentSignInRepository;
use crate::repository::person_sign_in::PersonSignInRepository;
use crate::repository::refresh_token_history::RefreshTokenHistoryRepository;

use crate::time::now_millis;

use super::access::PersonCaller;
use super::caller::{PersonCallerError, VerifiedCaller};
use super::token::{Claims, TokenKind, TokenSigner, TokenSubject};
use super::user_store::UserStore;
use super::{primary_key, random_id};

#[derive(Debug, Error)]
pub(crate) enum LoginError {
    #[error(transparent)]
    Caller(#[from] PersonCallerError),
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
    let (_, tokens) = create_sign_in(identity, &transaction, account).await?;
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
        if let Some(id) = history.agent_sign_in_id {
            AgentSignInRepository::builder()
                .connection(&transaction)
                .build()
                .delete_by_id(&id)
                .await?;
        }
        if let Some(id) = history.person_sign_in_id {
            PersonSignInRepository::builder()
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
    let repository = PersonSignInRepository::builder()
        .connection(transaction)
        .build();
    let sign_in = repository
        .by_refresh_hash(hash.expose())
        .await?
        .ok_or(LoginError::Unauthorized)?;
    let now = now_millis()?;
    let username = active_username(transaction, &sign_in, now).await?;
    let refresh_token = new_refresh_token()?;
    let tokens = issue_tokens(
        identity,
        TokenSubject::builder()
            .username(username.as_str())
            .random_id(sign_in.random_id.as_str())
            .kind(TokenKind::PersonSignIn)
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
            agent_sign_in_id: Set(None),
            person_sign_in_id: Set(Some(sign_in.id.clone())),
            expires_at: Set(sign_in.expires_at),
        })
        .await?;
    let mut model: ActiveModel = sign_in.into();
    model.refresh_token_hash = Set(sha256_hex(refresh_token.expose()));
    model.expires_at = Set(sign_in_expiry(identity, now));
    repository.update(model).await?;
    Ok(tokens)
}

pub(crate) async fn log_out(
    database: &DatabaseConnection,
    caller: VerifiedCaller,
) -> Result<(), LoginError> {
    let caller = caller.person()?;
    let transaction = begin_write(database).await?;
    let repository = PersonSignInRepository::builder()
        .connection(&transaction)
        .build();
    if repository
        .delete(&caller.person_sign_in_id, &caller.account_id)
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
) -> Result<PersonCaller, LoginError> {
    let now = now_millis()?;
    if claims.kind != TokenKind::PersonSignIn || claims.exp <= (now / 1000) as u64 {
        return Err(LoginError::Unauthorized);
    }
    let transaction = database.begin().await?;
    let sign_in = PersonSignInRepository::builder()
        .connection(&transaction)
        .build()
        .by_random_id(&claims.sid)
        .await?
        .ok_or(LoginError::Unauthorized)?;
    let username = active_username(&transaction, &sign_in, now).await?;
    if username != claims.sub {
        return Err(LoginError::Unauthorized);
    }
    transaction.commit().await?;
    Ok(PersonCaller::builder()
        .account_id(sign_in.account_id)
        .person_sign_in_id(sign_in.id)
        .build())
}

pub(crate) async fn create_sign_in(
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
            .kind(TokenKind::PersonSignIn)
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
        expires_at: Set(sign_in_expiry(identity, now)),
    };
    PersonSignInRepository::builder()
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

pub(crate) fn sign_in_expiry(identity: &Identity, now: i64) -> i64 {
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
    sign_in: &Model,
    now: i64,
) -> Result<String, LoginError> {
    if sign_in.expires_at <= now {
        return Err(LoginError::Unauthorized);
    }
    let account = AccountRepository::builder()
        .connection(connection)
        .build()
        .by_id(&sign_in.account_id)
        .await?
        .filter(|account| account.deleted_at.is_none())
        .ok_or(LoginError::Unauthorized)?;
    Ok(account.username)
}
