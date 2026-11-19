use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, text, text_null, text_uniq};

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            account(),
            oauth_client(),
            agent_authorization(),
            login_session(),
            refresh_token_history(),
            oauth_code(),
            device_sign_in_request(),
            participant(),
        ] {
            manager.create_table(table).await?;
        }
        for index in indexes() {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            Participant::Table.into_iden(),
            DeviceSignInRequest::Table.into_iden(),
            OauthCode::Table.into_iden(),
            RefreshTokenHistory::Table.into_iden(),
            LoginSession::Table.into_iden(),
            AgentAuthorization::Table.into_iden(),
            OauthClient::Table.into_iden(),
            Account::Table.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(table).to_owned())
                .await?;
        }
        Ok(())
    }
}

fn account() -> TableCreateStatement {
    Table::create()
        .table(Account::Table)
        .col(text(Account::Id).primary_key())
        .col(text_uniq(Account::Username))
        .col(big_integer_null(Account::DeletedAt))
        .col(big_integer(Account::CreatedAt))
        .to_owned()
}

fn oauth_client() -> TableCreateStatement {
    Table::create()
        .table(OauthClient::Table)
        .col(text(OauthClient::Id).primary_key())
        .col(text_uniq(OauthClient::ClientId))
        .col(text(OauthClient::ClientName))
        .col(text(OauthClient::Metadata))
        .col(big_integer(OauthClient::CreatedAt))
        .col(big_integer_null(OauthClient::FirstAuthorizedAt))
        .to_owned()
}

fn agent_authorization() -> TableCreateStatement {
    Table::create()
        .table(AgentAuthorization::Table)
        .col(text(AgentAuthorization::Id).primary_key())
        .col(text(AgentAuthorization::AccountId))
        .col(text(AgentAuthorization::ClientId))
        .col(text(AgentAuthorization::Name))
        .col(text_uniq(AgentAuthorization::RandomId))
        .col(text_uniq(AgentAuthorization::RefreshTokenHash))
        .col(big_integer(AgentAuthorization::RefreshExpiresAt))
        .col(big_integer(AgentAuthorization::CreatedAt))
        .index(
            Index::create()
                .unique()
                .col(AgentAuthorization::AccountId)
                .col(AgentAuthorization::Name),
        )
        .foreign_key(
            ForeignKey::create()
                .from(AgentAuthorization::Table, AgentAuthorization::AccountId)
                .to(Account::Table, Account::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(AgentAuthorization::Table, AgentAuthorization::ClientId)
                .to(OauthClient::Table, OauthClient::ClientId),
        )
        .to_owned()
}

fn login_session() -> TableCreateStatement {
    Table::create()
        .table(LoginSession::Table)
        .col(text(LoginSession::Id).primary_key())
        .col(text(LoginSession::AccountId))
        .col(text_uniq(LoginSession::RandomId))
        .col(text_uniq(LoginSession::RefreshTokenHash))
        .col(big_integer(LoginSession::CreatedAt))
        .col(big_integer(LoginSession::ExpiresAt))
        .foreign_key(
            ForeignKey::create()
                .from(LoginSession::Table, LoginSession::AccountId)
                .to(Account::Table, Account::Id),
        )
        .to_owned()
}

fn oauth_code() -> TableCreateStatement {
    Table::create()
        .table(OauthCode::Table)
        .col(text(OauthCode::Id).primary_key())
        .col(text_uniq(OauthCode::CodeHash))
        .col(text(OauthCode::AccountId))
        .col(text(OauthCode::ClientId))
        .col(text(OauthCode::RedirectUri))
        .col(text(OauthCode::CodeChallenge))
        .col(text_null(OauthCode::AgentName))
        .col(text_null(OauthCode::ReplacesAuthorizationId))
        .col(big_integer(OauthCode::CreatedAt))
        .col(big_integer(OauthCode::ExpiresAt))
        .col(big_integer_null(OauthCode::ExchangedAt))
        .col(text_null(OauthCode::AgentAuthorizationId))
        .col(text_null(OauthCode::LoginSessionId))
        .check(
            Expr::col(OauthCode::AgentAuthorizationId)
                .is_null()
                .or(Expr::col(OauthCode::LoginSessionId).is_null()),
        )
        .foreign_key(
            ForeignKey::create()
                .from(OauthCode::Table, OauthCode::AgentAuthorizationId)
                .to(AgentAuthorization::Table, AgentAuthorization::Id)
                .on_delete(ForeignKeyAction::SetNull),
        )
        .foreign_key(
            ForeignKey::create()
                .from(OauthCode::Table, OauthCode::LoginSessionId)
                .to(LoginSession::Table, LoginSession::Id)
                .on_delete(ForeignKeyAction::SetNull),
        )
        .foreign_key(
            ForeignKey::create()
                .from(OauthCode::Table, OauthCode::AccountId)
                .to(Account::Table, Account::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(OauthCode::Table, OauthCode::ReplacesAuthorizationId)
                .to(AgentAuthorization::Table, AgentAuthorization::Id)
                .on_delete(ForeignKeyAction::SetNull),
        )
        .to_owned()
}

fn participant() -> TableCreateStatement {
    Table::create()
        .table(Participant::Table)
        .col(text(Participant::Id).primary_key())
        .col(text(Participant::SpaceId))
        .col(text(Participant::AccountId))
        .col(big_integer(Participant::SpaceNumber))
        .col(text(Participant::Role))
        .col(big_integer(Participant::CreatedAt))
        .col(big_integer(Participant::UpdatedAt))
        .index(
            Index::create()
                .unique()
                .col(Participant::SpaceId)
                .col(Participant::AccountId),
        )
        .index(
            Index::create()
                .unique()
                .col(Participant::AccountId)
                .col(Participant::SpaceNumber),
        )
        .check(Expr::col(Participant::Role).is_in(["owner", "admin", "contributor", "viewer"]))
        .foreign_key(
            ForeignKey::create()
                .from(Participant::Table, Participant::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Participant::Table, Participant::AccountId)
                .to(Account::Table, Account::Id),
        )
        .to_owned()
}

fn refresh_token_history() -> TableCreateStatement {
    let mut table = Table::create();
    table
        .table(RefreshTokenHistory::Table)
        .col(text(RefreshTokenHistory::Id).primary_key())
        .col(text_uniq(RefreshTokenHistory::TokenHash))
        .col(text_null(RefreshTokenHistory::AgentAuthorizationId))
        .col(text_null(RefreshTokenHistory::LoginSessionId))
        .col(big_integer(RefreshTokenHistory::ExpiresAt))
        .check(
            Expr::col(RefreshTokenHistory::AgentAuthorizationId)
                .is_null()
                .ne(Expr::col(RefreshTokenHistory::LoginSessionId).is_null()),
        );
    for (owner, target) in [
        (
            RefreshTokenHistory::AgentAuthorizationId,
            AgentAuthorization::Table.into_iden(),
        ),
        (
            RefreshTokenHistory::LoginSessionId,
            LoginSession::Table.into_iden(),
        ),
    ] {
        table.foreign_key(
            ForeignKey::create()
                .from(RefreshTokenHistory::Table, owner)
                .to(target, Alias::new("id"))
                .on_delete(ForeignKeyAction::Cascade),
        );
    }
    table
}

fn device_sign_in_request() -> TableCreateStatement {
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
        .check(Expr::col(DeviceSignInRequest::Status).is_in(["pending", "approved", "declined"]))
        .foreign_key(
            ForeignKey::create()
                .from(DeviceSignInRequest::Table, DeviceSignInRequest::AccountId)
                .to(Account::Table, Account::Id),
        )
        .to_owned()
}

fn indexes() -> [IndexCreateStatement; 4] {
    [
        index(
            "participant_one_owner",
            Participant::Table,
            [Participant::SpaceId],
        )
        .unique()
        .and_where(Expr::col(Participant::Role).eq("owner"))
        .to_owned(),
        index(
            "refresh_token_history_agent",
            RefreshTokenHistory::Table,
            [RefreshTokenHistory::AgentAuthorizationId],
        ),
        index(
            "refresh_token_history_session",
            RefreshTokenHistory::Table,
            [RefreshTokenHistory::LoginSessionId],
        ),
        index(
            "refresh_token_history_expiry",
            RefreshTokenHistory::Table,
            [RefreshTokenHistory::ExpiresAt],
        ),
    ]
}

fn index<T: IntoIden>(
    name: &str,
    table: T,
    columns: impl IntoIterator<Item = T>,
) -> IndexCreateStatement {
    let mut index = Index::create();
    index.name(name).table(table.into_iden());
    for column in columns {
        index.col(column.into_iden());
    }
    index
}

#[derive(DeriveIden)]
enum Account {
    CreatedAt,
    DeletedAt,
    Id,
    Table,
    Username,
}

#[derive(DeriveIden)]
enum AgentAuthorization {
    AccountId,
    ClientId,
    CreatedAt,
    Id,
    Name,
    RandomId,
    RefreshExpiresAt,
    RefreshTokenHash,
    Table,
}

#[derive(DeriveIden)]
enum LoginSession {
    AccountId,
    CreatedAt,
    ExpiresAt,
    Id,
    RandomId,
    RefreshTokenHash,
    Table,
}

#[derive(DeriveIden)]
enum OauthClient {
    ClientId,
    ClientName,
    CreatedAt,
    FirstAuthorizedAt,
    Id,
    Metadata,
    Table,
}

#[derive(DeriveIden)]
enum OauthCode {
    AccountId,
    AgentAuthorizationId,
    AgentName,
    ClientId,
    CodeChallenge,
    CodeHash,
    CreatedAt,
    ExchangedAt,
    ExpiresAt,
    Id,
    LoginSessionId,
    RedirectUri,
    ReplacesAuthorizationId,
    Table,
}

#[derive(DeriveIden)]
enum Participant {
    AccountId,
    CreatedAt,
    Id,
    Role,
    SpaceId,
    SpaceNumber,
    Table,
    UpdatedAt,
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

#[derive(DeriveIden)]
enum RefreshTokenHistory {
    AgentAuthorizationId,
    ExpiresAt,
    Id,
    LoginSessionId,
    Table,
    TokenHash,
}

#[derive(DeriveIden)]
enum Space {
    Id,
    Table,
}
