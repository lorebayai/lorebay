use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::big_integer_null;

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ClientSchema::OauthClient)
                    .add_column(big_integer_null(ClientSchema::FirstAuthorizedAt))
                    .to_owned(),
            )
            .await?;
        let first_authorization = Query::select()
            .expr(
                Expr::col((
                    AuthorizationSchema::AgentAuthorization,
                    AuthorizationSchema::CreatedAt,
                ))
                .min(),
            )
            .from(AuthorizationSchema::AgentAuthorization)
            .and_where(
                Expr::col((
                    AuthorizationSchema::AgentAuthorization,
                    AuthorizationSchema::ClientId,
                ))
                .equals((ClientSchema::OauthClient, ClientSchema::ClientId)),
            )
            .to_owned();
        manager
            .get_connection()
            .execute(
                &Query::update()
                    .table(ClientSchema::OauthClient)
                    .value(ClientSchema::FirstAuthorizedAt, first_authorization)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(ClientSchema::OauthClient)
                    .drop_column(ClientSchema::FirstAuthorizedAt)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum AuthorizationSchema {
    AgentAuthorization,
    ClientId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum ClientSchema {
    ClientId,
    FirstAuthorizedAt,
    OauthClient,
}
