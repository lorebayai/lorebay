use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, text, text_null, text_uniq};

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(DeviceSignInRequest::Table)
                    .col(text(DeviceSignInRequest::Id).primary_key())
                    .col(text_uniq(DeviceSignInRequest::DeviceCodeHash))
                    .col(text_uniq(DeviceSignInRequest::UserCodeHash))
                    .col(text(DeviceSignInRequest::Status))
                    .col(text_null(DeviceSignInRequest::AccountId))
                    .col(big_integer(DeviceSignInRequest::CreatedAt))
                    .col(big_integer(DeviceSignInRequest::ExpiresAt))
                    .col(big_integer(DeviceSignInRequest::PollIntervalSeconds))
                    .col(big_integer_null(DeviceSignInRequest::LastPolledAt))
                    .check(
                        Expr::col(DeviceSignInRequest::Status)
                            .is_in(["pending", "approved", "declined"]),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(DeviceSignInRequest::Table, DeviceSignInRequest::AccountId)
                            .to(Account::Table, Account::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(DeviceSignInRequest::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Account {
    Id,
    Table,
}

#[derive(DeriveIden)]
enum DeviceSignInRequest {
    AccountId,
    CreatedAt,
    DeviceCodeHash,
    ExpiresAt,
    Id,
    LastPolledAt,
    PollIntervalSeconds,
    Status,
    Table,
    UserCodeHash,
}
