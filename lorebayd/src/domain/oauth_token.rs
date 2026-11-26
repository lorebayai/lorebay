use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use lorebay_lib_types::{Secret, sha256_hex};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, DbErr, Set, SqlErr,
    SqliteTransactionMode, TransactionOptions, TransactionTrait,
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::account::Model as Account;
use crate::entity::agent_sign_in::ActiveModel;
use crate::entity::oauth_code::Model as Code;
use crate::entity::{oauth_code, refresh_token_history};
use crate::repository::OAuthClientRepository;
use crate::repository::account::AccountRepository;
use crate::repository::agent_sign_in::AgentSignInRepository;
use crate::repository::oauth_code::OauthCodeRepository;
use crate::repository::person_sign_in::PersonSignInRepository;
use crate::repository::refresh_token_history::RefreshTokenHistoryRepository;

use crate::time::now_millis;

use super::authorization::{CLI_CLIENT_ID, validate_agent_name};
use super::device_sign_in::{DeviceError, exchange_device_code};
use super::person_sign_in::{
    self, Identity, LoginError, Tokens, issue_tokens, new_refresh_token, sign_in_expiry,
};
use super::token::{TokenKind, TokenSubject};
use super::{primary_key, random_id};

#[derive(Clone, Debug)]
pub(crate) enum TokenGrant {
    AuthorizationCode(CodeExchange),
    DeviceCode {
        client_id: String,
        device_code: Secret<String>,
    },
    RefreshToken(TokenRefresh),
}

#[derive(Debug, Error)]
pub(crate) enum TokenError {
    #[error("Agent name is already taken")]
    AgentNameTaken,
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Device(#[from] DeviceError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid grant")]
    InvalidGrant,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct CodeExchange {
    pub client_id: String,
    pub code: Secret<String>,
    pub redirect_uri: String,
    pub code_verifier: Secret<String>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TokenRefresh {
    pub client_id: String,
    pub refresh_token: Secret<String>,
}

impl From<LoginError> for TokenError {
    fn from(error: LoginError) -> Self {
        match error {
            LoginError::Caller(error) => Self::Internal(error.into()),
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
        TokenGrant::DeviceCode {
            client_id,
            device_code,
        } => Ok(exchange_device_code(identity, database, &client_id, &device_code).await?),
        TokenGrant::RefreshToken(input) => refresh_tokens(identity, database, input).await,
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
        .by_hash(&sha256_hex(input.code.expose()))
        .await?
        .ok_or(TokenError::InvalidGrant)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(input.code_verifier.expose().as_bytes()));
    if code.expires_at <= now
        || code.client_id != input.client_id
        || code.redirect_uri != input.redirect_uri
        || code.code_challenge != challenge
    {
        return Err(TokenError::InvalidGrant);
    }
    if code.exchanged_at.is_some() {
        end_code_sign_in(&transaction, &code).await?;
        transaction.commit().await?;
        return Err(TokenError::InvalidGrant);
    }
    let account = active_account(&transaction, &code.account_id).await?;
    let is_cli = code.client_id == CLI_CLIENT_ID;
    if !is_cli {
        validate_code_agent_name(&code)?;
    }
    if let Some(id) = &code.replaces_sign_in_id {
        AgentSignInRepository::builder()
            .connection(&transaction)
            .build()
            .delete(id, &code.account_id)
            .await?;
    }
    let (id, tokens) = if is_cli {
        person_sign_in::create_sign_in(identity, &transaction, account).await?
    } else {
        create_agent(identity, &transaction, code.clone(), account, now).await?
    };
    let mut model: oauth_code::ActiveModel = code.into();
    model.exchanged_at = Set(Some(now));
    model.replaces_sign_in_id = Set(None);
    if is_cli {
        model.person_sign_in_id = Set(Some(id));
    } else {
        model.agent_sign_in_id = Set(Some(id));
    }
    repository.update(model).await?;
    transaction.commit().await?;
    Ok(tokens)
}

fn validate_code_agent_name(code: &Code) -> Result<(), TokenError> {
    let name = code.agent_name.as_deref().ok_or(TokenError::InvalidGrant)?;
    validate_agent_name(name).map_err(|_| TokenError::InvalidGrant)
}

async fn create_agent(
    identity: &Identity,
    transaction: &DatabaseTransaction,
    code: Code,
    account: Account,
    now: i64,
) -> Result<(String, Tokens), TokenError> {
    let random_id = random_id().map_err(anyhow::Error::from)?;
    let refresh_token = new_refresh_token()?;
    let tokens = issue_tokens(
        identity,
        TokenSubject::builder()
            .username(account.username.as_str())
            .random_id(random_id.as_str())
            .kind(TokenKind::AgentSignIn)
            .build(),
        refresh_token.clone(),
        now,
    )?;
    let id = primary_key();
    let model = ActiveModel {
        id: Set(id.clone()),
        account_id: Set(account.id),
        client_id: Set(code.client_id.clone()),
        name: Set(code.agent_name.ok_or(TokenError::InvalidGrant)?),
        random_id: Set(random_id),
        refresh_token_hash: Set(sha256_hex(refresh_token.expose())),
        refresh_expires_at: Set(sign_in_expiry(identity, now)),
        created_at: Set(now),
    };
    AgentSignInRepository::builder()
        .connection(transaction)
        .build()
        .insert(model)
        .await
        .map_err(|error| match error.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(_)) => TokenError::AgentNameTaken,
            _ => TokenError::Database(error),
        })?;
    OAuthClientRepository::new(transaction)
        .mark_authorized(&code.client_id, now)
        .await?;
    Ok((id, tokens))
}

async fn refresh_agent(
    identity: &Identity,
    transaction: &DatabaseTransaction,
    input: TokenRefresh,
    hash: &Secret<String>,
) -> Result<Tokens, TokenError> {
    let repository = AgentSignInRepository::builder()
        .connection(transaction)
        .build();
    let now = now_millis()?;
    let sign_in = repository
        .by_refresh_hash(hash.expose())
        .await?
        .filter(|row| row.client_id == input.client_id && row.refresh_expires_at > now)
        .ok_or(TokenError::InvalidGrant)?;
    let account = active_account(transaction, &sign_in.account_id).await?;
    let refresh_token = new_refresh_token()?;
    let tokens = issue_tokens(
        identity,
        TokenSubject::builder()
            .username(account.username.as_str())
            .random_id(sign_in.random_id.as_str())
            .kind(TokenKind::AgentSignIn)
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
            agent_sign_in_id: Set(Some(sign_in.id.clone())),
            person_sign_in_id: Set(None),
            expires_at: Set(sign_in.refresh_expires_at),
        })
        .await?;
    let mut model: ActiveModel = sign_in.into();
    model.refresh_token_hash = Set(sha256_hex(refresh_token.expose()));
    model.refresh_expires_at = Set(sign_in_expiry(identity, now));
    repository.update(model).await?;
    Ok(tokens)
}

async fn refresh_tokens(
    identity: &Identity,
    database: &DatabaseConnection,
    input: TokenRefresh,
) -> Result<Tokens, TokenError> {
    let transaction = begin_write(database).await?;
    let hash = Secret::from(sha256_hex(input.refresh_token.expose()));
    let now = now_millis()?;
    let history = RefreshTokenHistoryRepository::builder()
        .connection(&transaction)
        .build()
        .by_hash(hash.expose(), now)
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
        return Err(TokenError::InvalidGrant);
    }
    let tokens = if input.client_id == CLI_CLIENT_ID {
        person_sign_in::rotate_refresh(identity, &transaction, &hash).await?
    } else {
        refresh_agent(identity, &transaction, input, &hash).await?
    };
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

async fn end_code_sign_in(
    transaction: &DatabaseTransaction,
    code: &Code,
) -> Result<(), TokenError> {
    if let Some(id) = &code.agent_sign_in_id {
        AgentSignInRepository::builder()
            .connection(transaction)
            .build()
            .delete(id, &code.account_id)
            .await?;
    }
    if let Some(id) = &code.person_sign_in_id {
        PersonSignInRepository::builder()
            .connection(transaction)
            .build()
            .delete(id, &code.account_id)
            .await?;
    }
    Ok(())
}
