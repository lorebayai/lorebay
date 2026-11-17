use std::net::IpAddr;

use anyhow::anyhow;
use lorebay_base::RateLimitError;
use sea_orm::{
    DatabaseConnection, DatabaseTransaction, DbErr, Set, SqlErr, SqliteTransactionMode,
    TransactionOptions, TransactionTrait,
};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::device_sign_in_request::{ActiveModel, Model};
use crate::repository::account::AccountRepository;
use crate::repository::device_sign_in_request::DeviceSignInRequestRepository;

use super::authorization::CLI_CLIENT_ID;
use super::caller::{CallerError, VerifiedCaller};
use super::login_session::{
    Identity, LoginError, Tokens, create_session, new_refresh_token, now_millis,
};
use super::primary_key;
use super::token::token_hash;

#[derive(Clone, Copy, Debug)]
pub(crate) enum DeviceDecision {
    Approve,
    Decline,
}

#[derive(Debug, Error)]
pub(crate) enum DeviceError {
    #[error("The request was declined")]
    AccessDenied,
    #[error("Authorization is pending")]
    AuthorizationPending,
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error("The request has expired")]
    ExpiredToken,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Only the built-in CLI client may request device sign-in")]
    InvalidClient,
    #[error("Invalid device code")]
    InvalidGrant,
    #[error("Unknown, expired or already decided user code")]
    InvalidUserCode,
    #[error(transparent)]
    Login(#[from] LoginError),
    #[error(transparent)]
    RateLimit(#[from] RateLimitError),
    #[error("Poll less frequently")]
    SlowDown,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct DeviceCodes {
    pub device_code: String,
    pub user_code: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct DeviceDetails {
    pub user_code: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct DeviceVerification {
    pub user_code: String,
    pub address: IpAddr,
}

pub(crate) async fn request_device_code(
    identity: &Identity,
    database: &DatabaseConnection,
    address: IpAddr,
    client_id: &str,
) -> Result<DeviceCodes, DeviceError> {
    identity.device_limiter.check(address)?;
    require_cli(client_id)?;
    let transaction = begin_write(database).await?;
    let repository = DeviceSignInRequestRepository::builder()
        .connection(&transaction)
        .build();
    let now = now_millis()?;
    for _ in 0..CODE_ALLOCATION_ATTEMPTS {
        let codes = DeviceCodes::builder()
            .device_code(new_refresh_token()?)
            .user_code(new_user_code()?)
            .build();
        let model = ActiveModel {
            id: Set(primary_key()),
            device_code_hash: Set(token_hash(&codes.device_code)),
            user_code_hash: Set(token_hash(&normalize_user_code(&codes.user_code))),
            status: Set(DeviceStatus::Pending.as_str().to_owned()),
            account_id: Set(None),
            created_at: Set(now),
            expires_at: Set(
                now + identity.config.device_request_lifetime_seconds as i64 * MILLIS_PER_SECOND
            ),
            poll_interval_seconds: Set(identity.config.device_poll_interval_seconds as i64),
            last_polled_at: Set(None),
        };
        match repository.insert(model).await {
            Ok(_) => {
                transaction.commit().await?;
                return Ok(codes);
            }
            Err(error) if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(anyhow!("could not allocate a unique device sign-in code").into())
}

pub(crate) async fn read_device_request(
    identity: &Identity,
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    input: DeviceVerification,
) -> Result<DeviceDetails, DeviceError> {
    caller.person()?;
    identity.device_verification_limiter.check(input.address)?;
    let normalized = normalize_user_code(&input.user_code);
    let now = now_millis()?;
    let row = DeviceSignInRequestRepository::builder()
        .connection(database)
        .build()
        .by_user_hash(&token_hash(&normalized))
        .await?
        .filter(|row| row.expires_at > now && row.status == DeviceStatus::Pending.as_str())
        .ok_or(DeviceError::InvalidUserCode)?;
    Ok(DeviceDetails::builder()
        .user_code(display_user_code(&normalized))
        .created_at(row.created_at)
        .build())
}

pub(crate) async fn decide_device_request(
    identity: &Identity,
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    input: DeviceVerification,
    decision: DeviceDecision,
) -> Result<(), DeviceError> {
    let caller = caller.person()?;
    identity.device_verification_limiter.check(input.address)?;
    let transaction = begin_write(database).await?;
    let repository = DeviceSignInRequestRepository::builder()
        .connection(&transaction)
        .build();
    let now = now_millis()?;
    let row = repository
        .by_user_hash(&token_hash(&normalize_user_code(&input.user_code)))
        .await?
        .filter(|row| row.expires_at > now && row.status == DeviceStatus::Pending.as_str())
        .ok_or(DeviceError::InvalidUserCode)?;
    let mut model: ActiveModel = row.into();
    let status = match decision {
        DeviceDecision::Approve => {
            model.account_id = Set(Some(caller.account_id.clone()));
            DeviceStatus::Approved
        }
        DeviceDecision::Decline => DeviceStatus::Declined,
    };
    model.status = Set(status.as_str().to_owned());
    repository.update(model).await?;
    transaction.commit().await?;
    Ok(())
}

pub(crate) async fn exchange_device_code(
    identity: &Identity,
    database: &DatabaseConnection,
    client_id: &str,
    device_code: &str,
) -> Result<Tokens, DeviceError> {
    require_cli(client_id)?;
    let transaction = begin_write(database).await?;
    let repository = DeviceSignInRequestRepository::builder()
        .connection(&transaction)
        .build();
    let row = repository
        .by_device_hash(&token_hash(device_code))
        .await?
        .ok_or(DeviceError::InvalidGrant)?;
    let now = now_millis()?;
    if row.expires_at <= now {
        return Err(DeviceError::ExpiredToken);
    }
    let status = DeviceStatus::try_from(row.status.as_str())?;
    if matches!(status, DeviceStatus::Declined) {
        return Err(DeviceError::AccessDenied);
    }
    let too_fast = row
        .last_polled_at
        .is_some_and(|last| now - last < row.poll_interval_seconds * MILLIS_PER_SECOND);
    if too_fast || matches!(status, DeviceStatus::Pending) {
        let interval = row.poll_interval_seconds;
        let mut model: ActiveModel = row.into();
        model.last_polled_at = Set(Some(now));
        if too_fast {
            model.poll_interval_seconds = Set(interval.saturating_add(SLOW_DOWN_SECONDS));
        }
        repository.update(model).await?;
        transaction.commit().await?;
        return Err(if too_fast {
            DeviceError::SlowDown
        } else {
            DeviceError::AuthorizationPending
        });
    }
    let tokens = complete_device_request(identity, &transaction, &row).await?;
    repository.delete(&row.id).await?;
    transaction.commit().await?;
    Ok(tokens)
}

const CODE_ALLOCATION_ATTEMPTS: usize = 5;
const MILLIS_PER_SECOND: i64 = 1000;
const SLOW_DOWN_SECONDS: i64 = 5;
const USER_CODE_ALPHABET: &[u8; 20] = b"BCDFGHJKLMNPQRSTVWXZ";
const USER_CODE_LENGTH: usize = 8;

#[derive(Clone, Copy, Debug)]
enum DeviceStatus {
    Approved,
    Declined,
    Pending,
}

impl DeviceStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::Declined => "declined",
            Self::Pending => "pending",
        }
    }
}

impl TryFrom<&str> for DeviceStatus {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "approved" => Ok(Self::Approved),
            "declined" => Ok(Self::Declined),
            "pending" => Ok(Self::Pending),
            _ => Err(anyhow!("invalid stored device sign-in status")),
        }
    }
}

async fn begin_write(database: &DatabaseConnection) -> Result<DatabaseTransaction, DbErr> {
    database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
}

async fn complete_device_request(
    identity: &Identity,
    transaction: &DatabaseTransaction,
    row: &Model,
) -> Result<Tokens, DeviceError> {
    let account_id = row.account_id.as_deref().ok_or(DeviceError::InvalidGrant)?;
    let account = AccountRepository::builder()
        .connection(transaction)
        .build()
        .by_id(account_id)
        .await?
        .filter(|account| account.deleted_at.is_none())
        .ok_or(DeviceError::InvalidGrant)?;
    let (_, tokens) = create_session(identity, transaction, account).await?;
    Ok(tokens)
}

fn require_cli(client_id: &str) -> Result<(), DeviceError> {
    if client_id != CLI_CLIENT_ID {
        return Err(DeviceError::InvalidClient);
    }
    Ok(())
}

fn normalize_user_code(code: &str) -> String {
    code.replace('-', "").to_ascii_uppercase()
}

fn display_user_code(normalized: &str) -> String {
    let middle = USER_CODE_LENGTH / 2;
    format!("{}-{}", &normalized[..middle], &normalized[middle..])
}

fn new_user_code() -> Result<String, DeviceError> {
    let mut code = String::with_capacity(USER_CODE_LENGTH);
    while code.len() < USER_CODE_LENGTH {
        let mut byte = [0];
        getrandom::fill(&mut byte).map_err(anyhow::Error::from)?;
        let limit = u8::MAX - u8::MAX % USER_CODE_ALPHABET.len() as u8;
        if byte[0] < limit {
            code.push(char::from(
                USER_CODE_ALPHABET[usize::from(byte[0]) % USER_CODE_ALPHABET.len()],
            ));
        }
    }
    Ok(display_user_code(&code))
}
