use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::text_null;

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (table, index) in tables() {
            manager
                .alter_table(
                    Table::alter()
                        .table(table.clone())
                        .add_column(text_null(RefreshToken::PreviousRefreshTokenHash))
                        .to_owned(),
                )
                .await?;
            manager
                .create_index(
                    Index::create()
                        .name(index)
                        .table(table)
                        .col(RefreshToken::PreviousRefreshTokenHash)
                        .unique()
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for (table, index) in tables().into_iter().rev() {
            manager
                .drop_index(Index::drop().name(index).table(table.clone()).to_owned())
                .await?;
            manager
                .alter_table(
                    Table::alter()
                        .table(table)
                        .drop_column(RefreshToken::PreviousRefreshTokenHash)
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }
}

#[derive(DeriveIden)]
enum RefreshToken {
    AgentAuthorization,
    LoginSession,
    PreviousRefreshTokenHash,
}

fn tables() -> [(DynIden, &'static str); 2] {
    [
        (
            RefreshToken::AgentAuthorization.into_iden(),
            "agent_authorization_previous_refresh_token_hash",
        ),
        (
            RefreshToken::LoginSession.into_iden(),
            "login_session_previous_refresh_token_hash",
        ),
    ]
}
