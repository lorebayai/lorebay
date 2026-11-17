use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, text, text_null, text_uniq};
use uuid::Uuid;

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(history()).await?;
        for (table, owner, expiry, index) in owners() {
            preserve_previous(manager, table.clone(), owner, expiry).await?;
            manager
                .drop_index(Index::drop().name(index).table(table.clone()).to_owned())
                .await?;
            manager
                .alter_table(
                    Table::alter()
                        .table(table)
                        .drop_column(Credential::PreviousRefreshTokenHash)
                        .to_owned(),
                )
                .await?;
        }
        for (name, column) in [
            (
                "refresh_token_history_agent",
                Credential::AgentAuthorizationId,
            ),
            ("refresh_token_history_session", Credential::LoginSessionId),
            ("refresh_token_history_expiry", Credential::ExpiresAt),
        ] {
            manager
                .create_index(
                    Index::create()
                        .name(name)
                        .table(Credential::RefreshTokenHistory)
                        .col(column)
                        .to_owned(),
                )
                .await?;
        }
        rebuild_codes(manager, CodeSchema::ReplayDetection).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rebuild_codes(manager, CodeSchema::Original).await?;
        for (table, _, _, index) in owners() {
            manager
                .alter_table(
                    Table::alter()
                        .table(table.clone())
                        .add_column(text_null(Credential::PreviousRefreshTokenHash))
                        .to_owned(),
                )
                .await?;
            manager
                .create_index(
                    Index::create()
                        .name(index)
                        .table(table)
                        .col(Credential::PreviousRefreshTokenHash)
                        .unique()
                        .to_owned(),
                )
                .await?;
        }
        manager
            .drop_table(
                Table::drop()
                    .table(Credential::RefreshTokenHistory)
                    .to_owned(),
            )
            .await
    }
}

const HISTORY_INSERT_BATCH_SIZE: usize = 256;

enum CodeSchema {
    Original,
    ReplayDetection,
}

#[derive(Clone, Copy, DeriveIden)]
enum Credential {
    Account,
    AccountId,
    AgentAuthorization,
    AgentAuthorizationId,
    AgentName,
    ClientId,
    CodeChallenge,
    CodeHash,
    CreatedAt,
    ExchangedAt,
    ExpiresAt,
    Id,
    LoginSession,
    LoginSessionId,
    OauthCode,
    OauthCodeBeforeRebuild,
    PreviousRefreshTokenHash,
    RedirectUri,
    RefreshExpiresAt,
    RefreshTokenHistory,
    ReplacesAuthorizationId,
    TokenHash,
}

fn owners() -> [(DynIden, Credential, Credential, &'static str); 2] {
    [
        (
            Credential::AgentAuthorization.into_iden(),
            Credential::AgentAuthorizationId,
            Credential::RefreshExpiresAt,
            "agent_authorization_previous_refresh_token_hash",
        ),
        (
            Credential::LoginSession.into_iden(),
            Credential::LoginSessionId,
            Credential::ExpiresAt,
            "login_session_previous_refresh_token_hash",
        ),
    ]
}

fn history() -> TableCreateStatement {
    let mut table = Table::create();
    table
        .table(Credential::RefreshTokenHistory)
        .col(text(Credential::Id).primary_key())
        .col(text_uniq(Credential::TokenHash))
        .col(text_null(Credential::AgentAuthorizationId))
        .col(text_null(Credential::LoginSessionId))
        .col(big_integer(Credential::ExpiresAt))
        .check(
            Expr::col(Credential::AgentAuthorizationId)
                .is_null()
                .ne(Expr::col(Credential::LoginSessionId).is_null()),
        );
    for (owner, target) in [
        (
            Credential::AgentAuthorizationId,
            Credential::AgentAuthorization,
        ),
        (Credential::LoginSessionId, Credential::LoginSession),
    ] {
        table.foreign_key(
            ForeignKey::create()
                .from(Credential::RefreshTokenHistory, owner)
                .to(target, Credential::Id)
                .on_delete(ForeignKeyAction::Cascade),
        );
    }
    table
}

async fn preserve_previous(
    manager: &SchemaManager<'_>,
    table: DynIden,
    owner: Credential,
    expiry: Credential,
) -> Result<(), DbErr> {
    let rows = manager
        .get_connection()
        .query_all(
            Query::select()
                .columns([Credential::Id, Credential::PreviousRefreshTokenHash, expiry])
                .from(table)
                .and_where(Expr::col(Credential::PreviousRefreshTokenHash).is_not_null()),
        )
        .await?;
    if rows.is_empty() {
        return Ok(());
    }
    for batch in rows.chunks(HISTORY_INSERT_BATCH_SIZE) {
        let mut insert = Query::insert();
        insert.into_table(Credential::RefreshTokenHistory).columns([
            Credential::Id,
            Credential::TokenHash,
            owner,
            Credential::ExpiresAt,
        ]);
        for row in batch {
            // Earlier schemas retain no expiry for the replaced token; preserve detection through the owner's expiry.
            insert
                .values([
                    Uuid::now_v7().to_string().into(),
                    row.try_get_by_index::<String>(1)?.into(),
                    row.try_get_by_index::<String>(0)?.into(),
                    row.try_get_by_index::<i64>(2)?.into(),
                ])
                .map_err(|error| DbErr::Migration(error.to_string()))?;
        }
        manager.get_connection().execute(&insert).await?;
    }
    Ok(())
}

async fn rebuild_codes(manager: &SchemaManager<'_>, schema: CodeSchema) -> Result<(), DbErr> {
    manager
        .rename_table(
            Table::rename()
                .table(Credential::OauthCode, Credential::OauthCodeBeforeRebuild)
                .to_owned(),
        )
        .await?;
    manager.create_table(codes(&schema)).await?;
    let columns = [
        Credential::Id,
        Credential::CodeHash,
        Credential::AccountId,
        Credential::ClientId,
        Credential::RedirectUri,
        Credential::CodeChallenge,
        Credential::AgentName,
        Credential::ReplacesAuthorizationId,
        Credential::CreatedAt,
        Credential::ExpiresAt,
    ];
    let mut select = Query::select();
    select
        .columns(columns)
        .from(Credential::OauthCodeBeforeRebuild);
    if matches!(schema, CodeSchema::Original) {
        select.and_where(Expr::col(Credential::ExchangedAt).is_null());
    }
    let insert = Query::insert()
        .into_table(Credential::OauthCode)
        .columns(columns)
        .select_from(select)
        .map_err(|error| DbErr::Migration(error.to_string()))?
        .to_owned();
    manager.get_connection().execute(&insert).await?;
    manager
        .drop_table(
            Table::drop()
                .table(Credential::OauthCodeBeforeRebuild)
                .to_owned(),
        )
        .await
}

fn codes(schema: &CodeSchema) -> TableCreateStatement {
    let mut table = Table::create();
    table
        .table(Credential::OauthCode)
        .col(text(Credential::Id).primary_key())
        .col(text_uniq(Credential::CodeHash))
        .col(text(Credential::AccountId))
        .col(text(Credential::ClientId))
        .col(text(Credential::RedirectUri))
        .col(text(Credential::CodeChallenge))
        .col(text_null(Credential::AgentName))
        .col(text_null(Credential::ReplacesAuthorizationId))
        .col(big_integer(Credential::CreatedAt))
        .col(big_integer(Credential::ExpiresAt))
        .foreign_key(
            ForeignKey::create()
                .from(Credential::OauthCode, Credential::AccountId)
                .to(Credential::Account, Credential::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Credential::OauthCode, Credential::ReplacesAuthorizationId)
                .to(Credential::AgentAuthorization, Credential::Id)
                .on_delete(ForeignKeyAction::SetNull),
        );
    if matches!(schema, CodeSchema::ReplayDetection) {
        table
            .col(big_integer_null(Credential::ExchangedAt))
            .col(text_null(Credential::AgentAuthorizationId))
            .col(text_null(Credential::LoginSessionId))
            .check(
                Expr::col(Credential::AgentAuthorizationId)
                    .is_null()
                    .or(Expr::col(Credential::LoginSessionId).is_null()),
            );
        for (owner, target) in [
            (
                Credential::AgentAuthorizationId,
                Credential::AgentAuthorization,
            ),
            (Credential::LoginSessionId, Credential::LoginSession),
        ] {
            table.foreign_key(
                ForeignKey::create()
                    .from(Credential::OauthCode, owner)
                    .to(target, Credential::Id)
                    .on_delete(ForeignKeyAction::SetNull),
            );
        }
    }
    table
}
