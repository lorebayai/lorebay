use std::collections::HashMap;

use futures_util::future::BoxFuture;
use lorebay_lib_core::{OperationHandle, PersonNames};

use crate::repository::account::AccountRepository;

pub(super) struct Names;

impl PersonNames for Names {
    fn names<'a>(
        &'a self,
        handle: OperationHandle<'a>,
        people: &'a [String],
    ) -> BoxFuture<'a, anyhow::Result<HashMap<String, String>>> {
        Box::pin(async move {
            let accounts = AccountRepository::builder()
                .connection(handle.transaction())
                .build()
                .by_ids(people)
                .await?;
            Ok(accounts
                .into_iter()
                .map(|account| (account.id, account.username))
                .collect())
        })
    }

    fn person_id<'a>(
        &'a self,
        handle: OperationHandle<'a>,
        name: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Option<String>>> {
        Box::pin(async move {
            let account = AccountRepository::builder()
                .connection(handle.transaction())
                .build()
                .by_username(name)
                .await?;
            Ok(account.map(|account| account.id))
        })
    }
}
