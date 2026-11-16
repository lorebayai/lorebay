use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{integer, integer_null, text, text_null, text_uniq};

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
            oauth_code(),
            space(),
            participant(),
            task(),
            note(),
            artifact(),
            knowledge(),
            proposal(),
            knowledge_version(),
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
            KnowledgeVersion::Table.into_iden(),
            Proposal::Table.into_iden(),
            Knowledge::Table.into_iden(),
            Artifact::Table.into_iden(),
            Note::Table.into_iden(),
            Task::Table.into_iden(),
            Participant::Table.into_iden(),
            Space::Table.into_iden(),
            OauthCode::Table.into_iden(),
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
        .col(ColumnDef::new(Account::Id).integer().primary_key())
        .col(text_uniq(Account::Username))
        .col(integer_null(Account::DeletedAt))
        .col(integer(Account::CreatedAt))
        .to_owned()
}

fn oauth_client() -> TableCreateStatement {
    Table::create()
        .table(OauthClient::Table)
        .col(ColumnDef::new(OauthClient::Id).integer().primary_key())
        .col(text_uniq(OauthClient::ClientId))
        .col(text(OauthClient::ClientName))
        .col(text(OauthClient::Metadata))
        .col(integer(OauthClient::CreatedAt))
        .to_owned()
}

fn agent_authorization() -> TableCreateStatement {
    Table::create()
        .table(AgentAuthorization::Table)
        .col(
            ColumnDef::new(AgentAuthorization::Id)
                .integer()
                .primary_key(),
        )
        .col(integer(AgentAuthorization::AccountId))
        .col(text(AgentAuthorization::ClientId))
        .col(text(AgentAuthorization::Name))
        .col(text_uniq(AgentAuthorization::RandomId))
        .col(text_uniq(AgentAuthorization::RefreshTokenHash))
        .col(integer(AgentAuthorization::RefreshExpiresAt))
        .col(integer(AgentAuthorization::CreatedAt))
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
        .col(ColumnDef::new(LoginSession::Id).integer().primary_key())
        .col(integer(LoginSession::AccountId))
        .col(text_uniq(LoginSession::RandomId))
        .col(text_uniq(LoginSession::RefreshTokenHash))
        .col(integer(LoginSession::CreatedAt))
        .col(integer(LoginSession::ExpiresAt))
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
        .col(ColumnDef::new(OauthCode::Id).integer().primary_key())
        .col(text_uniq(OauthCode::CodeHash))
        .col(integer(OauthCode::AccountId))
        .col(text(OauthCode::ClientId))
        .col(text(OauthCode::RedirectUri))
        .col(text(OauthCode::CodeChallenge))
        .col(text_null(OauthCode::AgentName))
        .col(integer_null(OauthCode::ReplacesAuthorizationId))
        .col(integer(OauthCode::CreatedAt))
        .col(integer(OauthCode::ExpiresAt))
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

fn space() -> TableCreateStatement {
    Table::create()
        .table(Space::Table)
        .col(
            ColumnDef::new(Space::Id)
                .integer()
                .primary_key()
                .auto_increment(),
        )
        .col(text(Space::Name))
        .col(text(Space::Summary))
        .col(integer(Space::NextTaskNumber).default(1))
        .col(integer(Space::NextKnowledgeNumber).default(1))
        .col(integer(Space::NextArtifactNumber).default(1))
        .col(integer(Space::NextProposalNumber).default(1))
        .col(integer(Space::CreatedAt))
        .col(integer(Space::UpdatedAt))
        .to_owned()
}

fn participant() -> TableCreateStatement {
    Table::create()
        .table(Participant::Table)
        .col(ColumnDef::new(Participant::Id).integer().primary_key())
        .col(integer(Participant::SpaceId))
        .col(integer(Participant::AccountId))
        .col(text(Participant::Role))
        .col(integer(Participant::CreatedAt))
        .col(integer(Participant::UpdatedAt))
        .index(
            Index::create()
                .unique()
                .col(Participant::SpaceId)
                .col(Participant::AccountId),
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

fn task() -> TableCreateStatement {
    Table::create()
        .table(Task::Table)
        .col(ColumnDef::new(Task::Id).integer().primary_key())
        .col(integer(Task::SpaceId))
        .col(integer(Task::Number))
        .col(text_uniq(Task::RandomId))
        .col(integer(Task::AccountId))
        .col(text(Task::AgentName))
        .col(text(Task::Title))
        .col(text(Task::Status))
        .col(text(Task::Result).default(""))
        .col(integer(Task::StartedAt))
        .col(integer_null(Task::EndedAt))
        .col(integer(Task::LastActivityAt))
        .index(
            Index::create()
                .unique()
                .col(Task::SpaceId)
                .col(Task::Number),
        )
        .index(Index::create().unique().col(Task::Id).col(Task::SpaceId))
        .check(Expr::col(Task::Status).is_in(["in progress", "ended"]))
        .check(
            Expr::col(Task::Status)
                .eq("ended")
                .eq(Expr::col(Task::EndedAt).is_not_null()),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Task::Table, Task::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Task::Table, Task::AccountId)
                .to(Account::Table, Account::Id),
        )
        .to_owned()
}

fn note() -> TableCreateStatement {
    Table::create()
        .table(Note::Table)
        .col(ColumnDef::new(Note::Id).integer().primary_key())
        .col(integer(Note::TaskId))
        .col(text(Note::Content))
        .col(integer(Note::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .from(Note::Table, Note::TaskId)
                .to(Task::Table, Task::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn artifact() -> TableCreateStatement {
    Table::create()
        .table(Artifact::Table)
        .col(ColumnDef::new(Artifact::Id).integer().primary_key())
        .col(integer(Artifact::SpaceId))
        .col(integer(Artifact::TaskId))
        .col(integer(Artifact::Number))
        .col(text(Artifact::FileName))
        .col(text(Artifact::Summary))
        .col(text_uniq(Artifact::StorageKey))
        .col(integer_null(Artifact::Size))
        .col(integer(Artifact::CreatedAt))
        .col(integer_null(Artifact::CompletedAt))
        .index(
            Index::create()
                .unique()
                .col(Artifact::SpaceId)
                .col(Artifact::Number),
        )
        .check(
            Expr::col(Artifact::CompletedAt)
                .is_null()
                .eq(Expr::col(Artifact::Size).is_null()),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Artifact::Table, Artifact::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Artifact::Table, (Artifact::TaskId, Artifact::SpaceId))
                .to(Task::Table, (Task::Id, Task::SpaceId))
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn knowledge() -> TableCreateStatement {
    Table::create()
        .table(Knowledge::Table)
        .col(ColumnDef::new(Knowledge::Id).integer().primary_key())
        .col(integer(Knowledge::SpaceId))
        .col(integer(Knowledge::Number))
        .col(text(Knowledge::Name))
        .col(text(Knowledge::NameKey))
        .col(integer(Knowledge::LatestVersion))
        .col(integer_null(Knowledge::DeletedAt))
        .col(integer(Knowledge::CreatedAt))
        .col(integer(Knowledge::UpdatedAt))
        .index(
            Index::create()
                .unique()
                .col(Knowledge::SpaceId)
                .col(Knowledge::Number),
        )
        .index(
            Index::create()
                .unique()
                .col(Knowledge::SpaceId)
                .col(Knowledge::NameKey),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Knowledge::Table, Knowledge::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .to_owned()
}

fn proposal() -> TableCreateStatement {
    let mut table = Table::create()
        .table(Proposal::Table)
        .col(ColumnDef::new(Proposal::Id).integer().primary_key())
        .col(integer(Proposal::SpaceId))
        .col(integer(Proposal::Number))
        .col(text(Proposal::Kind))
        .col(integer_null(Proposal::KnowledgeId))
        .col(integer_null(Proposal::BaseVersion))
        .col(text_null(Proposal::NewName))
        .col(text_null(Proposal::NewSummary))
        .col(text_null(Proposal::NewContent))
        .col(integer(Proposal::ProposerAccountId))
        .col(text_null(Proposal::ProposerAgentName))
        .col(integer_null(Proposal::FromTaskId))
        .col(text(Proposal::Status))
        .col(integer_null(Proposal::DecidedBy))
        .col(integer_null(Proposal::DecidedAt))
        .col(text_null(Proposal::RejectReason))
        .col(integer(Proposal::CreatedAt))
        .index(
            Index::create()
                .unique()
                .col(Proposal::SpaceId)
                .col(Proposal::Number),
        )
        .to_owned();
    proposal_checks(&mut table);
    proposal_foreign_keys(&mut table);
    table
}

fn proposal_checks(table: &mut TableCreateStatement) {
    table
        .check(Expr::col(Proposal::Kind).is_in(["create", "change", "delete"]))
        .check(Expr::col(Proposal::Status).is_in(["pending", "approved", "rejected"]))
        .check(proposal_kind())
        .check(
            Cond::any()
                .add(
                    Cond::all()
                        .add(Expr::col(Proposal::Status).eq("pending"))
                        .add(Expr::col(Proposal::DecidedBy).is_null())
                        .add(Expr::col(Proposal::DecidedAt).is_null()),
                )
                .add(
                    Cond::all()
                        .add(Expr::col(Proposal::Status).ne("pending"))
                        .add(Expr::col(Proposal::DecidedBy).is_not_null())
                        .add(Expr::col(Proposal::DecidedAt).is_not_null()),
                ),
        )
        .check(
            Cond::any()
                .add(Expr::col(Proposal::Status).ne("rejected"))
                .add(Expr::col(Proposal::RejectReason).is_not_null()),
        );
}

fn proposal_foreign_keys(table: &mut TableCreateStatement) {
    table
        .foreign_key(
            ForeignKey::create()
                .from(Proposal::Table, Proposal::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Proposal::Table, Proposal::KnowledgeId)
                .to(Knowledge::Table, Knowledge::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Proposal::Table, Proposal::ProposerAccountId)
                .to(Account::Table, Account::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Proposal::Table, Proposal::FromTaskId)
                .to(Task::Table, Task::Id)
                .on_delete(ForeignKeyAction::SetNull),
        )
        .foreign_key(
            ForeignKey::create()
                .from(Proposal::Table, Proposal::DecidedBy)
                .to(Account::Table, Account::Id),
        );
}

fn proposal_kind() -> Condition {
    Cond::any()
        .add(
            Cond::all()
                .add(Expr::col(Proposal::Kind).eq("create"))
                .add(Expr::col(Proposal::BaseVersion).is_null())
                .add(Expr::col(Proposal::NewName).is_not_null())
                .add(Expr::col(Proposal::NewSummary).is_not_null())
                .add(Expr::col(Proposal::NewContent).is_not_null()),
        )
        .add(
            Cond::all()
                .add(Expr::col(Proposal::Kind).eq("change"))
                .add(Expr::col(Proposal::KnowledgeId).is_not_null())
                .add(Expr::col(Proposal::BaseVersion).is_not_null())
                .add(
                    Cond::any()
                        .add(Expr::col(Proposal::NewName).is_not_null())
                        .add(Expr::col(Proposal::NewSummary).is_not_null())
                        .add(Expr::col(Proposal::NewContent).is_not_null()),
                ),
        )
        .add(
            Cond::all()
                .add(Expr::col(Proposal::Kind).eq("delete"))
                .add(Expr::col(Proposal::KnowledgeId).is_not_null())
                .add(Expr::col(Proposal::BaseVersion).is_not_null())
                .add(Expr::col(Proposal::NewName).is_null())
                .add(Expr::col(Proposal::NewSummary).is_null())
                .add(Expr::col(Proposal::NewContent).is_null()),
        )
}

fn knowledge_version() -> TableCreateStatement {
    Table::create()
        .table(KnowledgeVersion::Table)
        .col(ColumnDef::new(KnowledgeVersion::Id).integer().primary_key())
        .col(integer(KnowledgeVersion::KnowledgeId))
        .col(integer(KnowledgeVersion::Version))
        .col(text(KnowledgeVersion::Kind))
        .col(text_null(KnowledgeVersion::Content))
        .col(text_null(KnowledgeVersion::Summary))
        .col(integer(KnowledgeVersion::AppliedBy))
        .col(integer_null(KnowledgeVersion::ProposalId))
        .col(integer(KnowledgeVersion::CreatedAt))
        .index(
            Index::create()
                .unique()
                .col(KnowledgeVersion::KnowledgeId)
                .col(KnowledgeVersion::Version),
        )
        .check(Expr::col(KnowledgeVersion::Kind).is_in(["content", "deletion"]))
        .check(
            Cond::any()
                .add(
                    Cond::all()
                        .add(Expr::col(KnowledgeVersion::Kind).eq("content"))
                        .add(Expr::col(KnowledgeVersion::Content).is_not_null())
                        .add(Expr::col(KnowledgeVersion::Summary).is_not_null()),
                )
                .add(
                    Cond::all()
                        .add(Expr::col(KnowledgeVersion::Kind).eq("deletion"))
                        .add(Expr::col(KnowledgeVersion::Content).is_null())
                        .add(Expr::col(KnowledgeVersion::Summary).is_null()),
                ),
        )
        .foreign_key(
            ForeignKey::create()
                .from(KnowledgeVersion::Table, KnowledgeVersion::KnowledgeId)
                .to(Knowledge::Table, Knowledge::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(KnowledgeVersion::Table, KnowledgeVersion::AppliedBy)
                .to(Account::Table, Account::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(KnowledgeVersion::Table, KnowledgeVersion::ProposalId)
                .to(Proposal::Table, Proposal::Id),
        )
        .to_owned()
}

fn indexes() -> [IndexCreateStatement; 8] {
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
            "participant_account",
            Participant::Table,
            [Participant::AccountId],
        ),
        index(
            "task_space_activity",
            Task::Table,
            [Task::SpaceId, Task::LastActivityAt],
        ),
        index(
            "task_space_account",
            Task::Table,
            [Task::SpaceId, Task::AccountId, Task::Status],
        ),
        index(
            "note_task_time",
            Note::Table,
            [Note::TaskId, Note::CreatedAt],
        ),
        index("artifact_task", Artifact::Table, [Artifact::TaskId]),
        index(
            "proposal_space_status",
            Proposal::Table,
            [Proposal::SpaceId, Proposal::Status, Proposal::CreatedAt],
        ),
        index(
            "proposal_knowledge",
            Proposal::Table,
            [Proposal::KnowledgeId],
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
enum Artifact {
    CompletedAt,
    CreatedAt,
    FileName,
    Id,
    Number,
    Size,
    SpaceId,
    StorageKey,
    Summary,
    Table,
    TaskId,
}

#[derive(DeriveIden)]
enum Knowledge {
    CreatedAt,
    DeletedAt,
    Id,
    LatestVersion,
    Name,
    NameKey,
    Number,
    SpaceId,
    Table,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum KnowledgeVersion {
    AppliedBy,
    Content,
    CreatedAt,
    Id,
    Kind,
    KnowledgeId,
    ProposalId,
    Summary,
    Table,
    Version,
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
enum Note {
    Content,
    CreatedAt,
    Id,
    Table,
    TaskId,
}

#[derive(DeriveIden)]
enum OauthClient {
    ClientId,
    ClientName,
    CreatedAt,
    Id,
    Metadata,
    Table,
}

#[derive(DeriveIden)]
enum OauthCode {
    AccountId,
    AgentName,
    ClientId,
    CodeChallenge,
    CodeHash,
    CreatedAt,
    ExpiresAt,
    Id,
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
    Table,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Proposal {
    BaseVersion,
    CreatedAt,
    DecidedAt,
    DecidedBy,
    FromTaskId,
    Id,
    Kind,
    KnowledgeId,
    NewContent,
    NewName,
    NewSummary,
    Number,
    ProposerAccountId,
    ProposerAgentName,
    RejectReason,
    SpaceId,
    Status,
    Table,
}

#[derive(DeriveIden)]
enum Space {
    CreatedAt,
    Id,
    Name,
    NextArtifactNumber,
    NextKnowledgeNumber,
    NextProposalNumber,
    NextTaskNumber,
    Summary,
    Table,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Task {
    AccountId,
    AgentName,
    EndedAt,
    Id,
    LastActivityAt,
    Number,
    RandomId,
    Result,
    SpaceId,
    StartedAt,
    Status,
    Table,
    Title,
}
