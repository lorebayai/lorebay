use futures_util::future::BoxFuture;
use lorebay_lib_core::{DatabaseTransactions, WriteRecord};
use sea_orm::{
    DatabaseConnection, DatabaseTransaction, SqliteTransactionMode, TransactionOptions,
    TransactionTrait,
};

pub(super) struct Transactions(pub DatabaseConnection);

impl DatabaseTransactions for Transactions {
    fn begin_read(&self) -> BoxFuture<'_, anyhow::Result<DatabaseTransaction>> {
        Box::pin(async { Ok(self.0.begin().await?) })
    }

    fn end_read(&self, transaction: DatabaseTransaction) -> BoxFuture<'_, anyhow::Result<()>> {
        Box::pin(async move { Ok(transaction.commit().await?) })
    }

    fn begin_write(&self) -> BoxFuture<'_, anyhow::Result<DatabaseTransaction>> {
        Box::pin(async {
            Ok(self
                .0
                .begin_with_options(TransactionOptions {
                    sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
                    ..Default::default()
                })
                .await?)
        })
    }

    fn commit_write<'a>(
        &'a self,
        transaction: DatabaseTransaction,
        _record: Option<WriteRecord<'a>>,
    ) -> BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async move { Ok(transaction.commit().await?) })
    }

    fn abandon_write(&self, transaction: DatabaseTransaction) -> BoxFuture<'_, anyhow::Result<()>> {
        Box::pin(async move { Ok(transaction.rollback().await?) })
    }
}
