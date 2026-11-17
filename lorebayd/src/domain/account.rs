use std::collections::BTreeSet;

use anyhow::Result;
use sea_orm::{
    DatabaseConnection, DatabaseTransaction, Set, SqliteTransactionMode, TransactionOptions,
    TransactionTrait,
};

use crate::entity::account::ActiveModel;
use crate::repository::account::AccountRepository;
use crate::repository::agent_authorization::AgentAuthorizationRepository;
use crate::repository::device_sign_in_request::DeviceSignInRequestRepository;
use crate::repository::login_session::LoginSessionRepository;
use crate::repository::oauth_code::OauthCodeRepository;
use crate::repository::participant::ParticipantRepository;

use super::primary_key;
use super::user_store::UserStore;

pub(crate) async fn sync(
    database: &DatabaseConnection,
    users: &dyn UserStore,
    now: i64,
) -> Result<()> {
    let usernames: BTreeSet<_> = users.usernames()?.into_iter().collect();
    let transaction = database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await?;
    let repository = AccountRepository::builder()
        .connection(&transaction)
        .build();
    let accounts = repository.all().await?;
    let removed: Vec<_> = accounts
        .iter()
        .filter(|account| account.deleted_at.is_none() && !usernames.contains(&account.username))
        .map(|account| account.id.clone())
        .collect();
    let restored: Vec<_> = accounts
        .iter()
        .filter(|account| account.deleted_at.is_some() && usernames.contains(&account.username))
        .map(|account| account.id.clone())
        .collect();
    let new = usernames
        .iter()
        .filter(|username| {
            !accounts
                .iter()
                .any(|account| &account.username == *username)
        })
        .map(|username| ActiveModel {
            id: Set(primary_key()),
            username: Set(username.clone()),
            deleted_at: Set(None),
            created_at: Set(now),
        })
        .collect();
    repository.insert_many(new).await?;
    repository.set_deleted_at(&restored, None).await?;
    repository.set_deleted_at(&removed, Some(now)).await?;
    remove_credentials(&transaction, &removed).await?;
    transaction.commit().await?;
    Ok(())
}

async fn remove_credentials(transaction: &DatabaseTransaction, ids: &[String]) -> Result<()> {
    DeviceSignInRequestRepository::builder()
        .connection(transaction)
        .build()
        .delete_by_accounts(ids)
        .await?;
    ParticipantRepository::builder()
        .connection(transaction)
        .build()
        .delete_by_accounts(ids)
        .await?;
    OauthCodeRepository::builder()
        .connection(transaction)
        .build()
        .delete_by_accounts(ids)
        .await?;
    AgentAuthorizationRepository::builder()
        .connection(transaction)
        .build()
        .delete_by_accounts(ids)
        .await?;
    LoginSessionRepository::builder()
        .connection(transaction)
        .build()
        .delete_by_accounts(ids)
        .await?;
    Ok(())
}
