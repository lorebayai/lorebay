use std::num::TryFromIntError;
use std::time::{SystemTimeError, UNIX_EPOCH};

use lorebay_base::Clock;
use sea_orm::{
    DatabaseConnection, DbErr, Set, SqliteTransactionMode, TransactionOptions, TransactionTrait,
};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::domain::{primary_key, random_id};
use crate::entity::oauth_client::ActiveModel;
use crate::repository::OAuthClientRepository;

#[derive(Debug, Error)]
pub(crate) enum OAuthClientError {
    #[error(transparent)]
    Clock(#[from] SystemTimeError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Random(#[from] getrandom::Error),
    #[error(transparent)]
    Timestamp(#[from] TryFromIntError),
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct NewOAuthClient {
    pub client_name: String,
    pub metadata: String,
}

pub(crate) async fn register_client(
    database: &DatabaseConnection,
    input: NewOAuthClient,
    clock: &impl Clock,
) -> Result<String, OAuthClientError> {
    let client_id = random_id()?;
    let model = ActiveModel {
        id: Set(primary_key()),
        client_id: Set(client_id.clone()),
        client_name: Set(input.client_name),
        metadata: Set(input.metadata),
        created_at: Set(i64::try_from(
            clock.now().duration_since(UNIX_EPOCH)?.as_millis(),
        )?),
        first_authorized_at: Set(None),
    };
    let transaction = database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await?;
    OAuthClientRepository::new(&transaction)
        .insert(model)
        .await?;
    transaction.commit().await?;
    Ok(client_id)
}
