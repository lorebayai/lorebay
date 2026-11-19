use sea_orm::{DatabaseConnection, DbErr};
use thiserror::Error;

use crate::repository::account::AccountRepository;

use super::caller::{PersonSessionError, VerifiedCaller};
use super::pagination::{Page, PageOptions, decode_cursor, encode_cursor};

#[derive(Debug, Error)]
pub(crate) enum UserDirectoryError {
    #[error(transparent)]
    Caller(#[from] PersonSessionError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error("Invalid cursor")]
    InvalidCursor,
}

pub(crate) async fn list_users(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    options: PageOptions,
) -> Result<Page<String>, UserDirectoryError> {
    caller.person()?;
    let after = options
        .cursor
        .map(|cursor| {
            decode_cursor(CURSOR_VERSION, caller.account_id(), &cursor)
                .ok_or(UserDirectoryError::InvalidCursor)
        })
        .transpose()?;
    let mut records = AccountRepository::builder()
        .connection(database)
        .build()
        .list_active(after.as_deref(), options.page_size + 1)
        .await?;
    let has_more = records.len() as u64 > options.page_size;
    records.truncate(options.page_size as usize);
    let next_cursor = records
        .last()
        .filter(|_| has_more)
        .map(|record| encode_cursor(CURSOR_VERSION, caller.account_id(), &record.username));
    Ok(Page::builder()
        .items(
            records
                .into_iter()
                .map(|record| record.username)
                .collect::<Vec<_>>(),
        )
        .next_cursor(next_cursor)
        .build())
}

const CURSOR_VERSION: &str = "users-v1";
