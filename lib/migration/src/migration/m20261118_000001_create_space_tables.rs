use sea_orm_migration::prelude::*;
use sea_orm_migration::schema::{big_integer, big_integer_null, text, text_null, text_uniq};
use sea_orm_migration::sea_orm::DbBackend;

#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for table in [
            space(),
            task(),
            note(),
            artifact(),
            knowledge(manager.get_database_backend()),
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
            SpaceKnowledgeVersion::Table.into_iden(),
            SpaceProposal::Table.into_iden(),
            SpaceKnowledge::Table.into_iden(),
            SpaceArtifact::Table.into_iden(),
            SpaceNote::Table.into_iden(),
            SpaceTask::Table.into_iden(),
            Space::Table.into_iden(),
        ] {
            manager
                .drop_table(Table::drop().table(table).to_owned())
                .await?;
        }
        Ok(())
    }
}

fn space() -> TableCreateStatement {
    Table::create()
        .table(Space::Table)
        .col(text(Space::Id).primary_key())
        .col(big_integer(Space::Number).unique_key())
        .col(text(Space::OwnerId))
        .col(text(Space::Name))
        .col(text(Space::Summary))
        .col(big_integer(Space::NextTaskNumber).default(1))
        .col(big_integer(Space::NextKnowledgeNumber).default(1))
        .col(big_integer(Space::NextArtifactNumber).default(1))
        .col(big_integer(Space::NextProposalNumber).default(1))
        .col(big_integer(Space::CreatedAt))
        .col(big_integer(Space::UpdatedAt))
        .to_owned()
}

fn task() -> TableCreateStatement {
    Table::create()
        .table(SpaceTask::Table)
        .col(text(SpaceTask::Id).primary_key())
        .col(text(SpaceTask::SpaceId))
        .col(big_integer(SpaceTask::Number))
        .col(text_uniq(SpaceTask::RandomId))
        .col(text(SpaceTask::PersonId))
        .col(text(SpaceTask::AgentName))
        .col(text(SpaceTask::Title))
        .col(text(SpaceTask::Status))
        .col(text(SpaceTask::Result).default(""))
        .col(big_integer(SpaceTask::StartedAt))
        .col(big_integer_null(SpaceTask::EndedAt))
        .col(big_integer(SpaceTask::LastActivityAt))
        .index(
            Index::create()
                .unique()
                .col(SpaceTask::SpaceId)
                .col(SpaceTask::Number),
        )
        .index(
            Index::create()
                .unique()
                .col(SpaceTask::Id)
                .col(SpaceTask::SpaceId),
        )
        .check(Expr::col(SpaceTask::Status).is_in(["in progress", "ended"]))
        .check(
            Expr::col(SpaceTask::Status)
                .eq("ended")
                .eq(Expr::col(SpaceTask::EndedAt).is_not_null()),
        )
        .foreign_key(
            ForeignKey::create()
                .from(SpaceTask::Table, SpaceTask::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .to_owned()
}

fn note() -> TableCreateStatement {
    Table::create()
        .table(SpaceNote::Table)
        .col(text(SpaceNote::Id).primary_key())
        .col(text(SpaceNote::TaskId))
        .col(text(SpaceNote::Content))
        .col(big_integer(SpaceNote::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .from(SpaceNote::Table, SpaceNote::TaskId)
                .to(SpaceTask::Table, SpaceTask::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn artifact() -> TableCreateStatement {
    Table::create()
        .table(SpaceArtifact::Table)
        .col(text(SpaceArtifact::Id).primary_key())
        .col(text(SpaceArtifact::SpaceId))
        .col(text(SpaceArtifact::TaskId))
        .col(big_integer(SpaceArtifact::Number))
        .col(text(SpaceArtifact::FileName))
        .col(text(SpaceArtifact::Summary))
        .col(text_uniq(SpaceArtifact::StorageKey))
        .col(big_integer_null(SpaceArtifact::Size))
        .col(big_integer(SpaceArtifact::CreatedAt))
        .col(big_integer_null(SpaceArtifact::CompletedAt))
        .col(big_integer_null(SpaceArtifact::RefusedAt))
        .index(
            Index::create()
                .unique()
                .col(SpaceArtifact::SpaceId)
                .col(SpaceArtifact::Number),
        )
        .check(
            Expr::col(SpaceArtifact::CompletedAt)
                .is_null()
                .eq(Expr::col(SpaceArtifact::Size).is_null()),
        )
        .check(
            Expr::col(SpaceArtifact::RefusedAt)
                .is_null()
                .or(Expr::col(SpaceArtifact::CompletedAt).is_null()),
        )
        .foreign_key(
            ForeignKey::create()
                .from(SpaceArtifact::Table, SpaceArtifact::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(
                    SpaceArtifact::Table,
                    (SpaceArtifact::TaskId, SpaceArtifact::SpaceId),
                )
                .to(SpaceTask::Table, (SpaceTask::Id, SpaceTask::SpaceId))
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn knowledge(backend: DbBackend) -> TableCreateStatement {
    let mut name_key = text(SpaceKnowledge::NameKey);
    if backend == DbBackend::Postgres {
        name_key.extra("COLLATE \"C\"");
    }
    Table::create()
        .table(SpaceKnowledge::Table)
        .col(text(SpaceKnowledge::Id).primary_key())
        .col(text(SpaceKnowledge::SpaceId))
        .col(big_integer(SpaceKnowledge::Number))
        .col(text(SpaceKnowledge::Name))
        .col(name_key)
        .col(big_integer(SpaceKnowledge::LatestVersion))
        .col(big_integer_null(SpaceKnowledge::DeletedAt))
        .col(big_integer(SpaceKnowledge::CreatedAt))
        .col(big_integer(SpaceKnowledge::UpdatedAt))
        .index(
            Index::create()
                .unique()
                .col(SpaceKnowledge::SpaceId)
                .col(SpaceKnowledge::Number),
        )
        .index(
            Index::create()
                .unique()
                .col(SpaceKnowledge::SpaceId)
                .col(SpaceKnowledge::NameKey),
        )
        .foreign_key(
            ForeignKey::create()
                .from(SpaceKnowledge::Table, SpaceKnowledge::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .to_owned()
}

fn proposal() -> TableCreateStatement {
    let mut table = Table::create()
        .table(SpaceProposal::Table)
        .col(text(SpaceProposal::Id).primary_key())
        .col(text(SpaceProposal::SpaceId))
        .col(big_integer(SpaceProposal::Number))
        .col(text(SpaceProposal::Kind))
        .col(text_null(SpaceProposal::KnowledgeId))
        .col(big_integer_null(SpaceProposal::BaseVersion))
        .col(text_null(SpaceProposal::NewName))
        .col(text_null(SpaceProposal::NewSummary))
        .col(text_null(SpaceProposal::NewContent))
        .col(text(SpaceProposal::ProposedBy))
        .col(text_null(SpaceProposal::ProposerAgentName))
        .col(text_null(SpaceProposal::FromTaskId))
        .col(text(SpaceProposal::Status))
        .col(text_null(SpaceProposal::DecidedBy))
        .col(big_integer_null(SpaceProposal::DecidedAt))
        .col(text_null(SpaceProposal::RejectReason))
        .col(big_integer(SpaceProposal::CreatedAt))
        .index(
            Index::create()
                .unique()
                .col(SpaceProposal::SpaceId)
                .col(SpaceProposal::Number),
        )
        .to_owned();
    proposal_checks(&mut table);
    proposal_foreign_keys(&mut table);
    table
}

fn proposal_checks(table: &mut TableCreateStatement) {
    table
        .check(Expr::col(SpaceProposal::Kind).is_in(["create", "change", "delete"]))
        .check(Expr::col(SpaceProposal::Status).is_in(["pending", "approved", "rejected"]))
        .check(proposal_kind())
        .check(
            Cond::any()
                .add(
                    Cond::all()
                        .add(Expr::col(SpaceProposal::Status).eq("pending"))
                        .add(Expr::col(SpaceProposal::DecidedBy).is_null())
                        .add(Expr::col(SpaceProposal::DecidedAt).is_null()),
                )
                .add(
                    Cond::all()
                        .add(Expr::col(SpaceProposal::Status).ne("pending"))
                        .add(Expr::col(SpaceProposal::DecidedBy).is_not_null())
                        .add(Expr::col(SpaceProposal::DecidedAt).is_not_null()),
                ),
        )
        .check(
            Cond::any()
                .add(Expr::col(SpaceProposal::Status).ne("rejected"))
                .add(Expr::col(SpaceProposal::RejectReason).is_not_null()),
        );
}

fn proposal_foreign_keys(table: &mut TableCreateStatement) {
    table
        .foreign_key(
            ForeignKey::create()
                .from(SpaceProposal::Table, SpaceProposal::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(SpaceProposal::Table, SpaceProposal::KnowledgeId)
                .to(SpaceKnowledge::Table, SpaceKnowledge::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(SpaceProposal::Table, SpaceProposal::FromTaskId)
                .to(SpaceTask::Table, SpaceTask::Id)
                .on_delete(ForeignKeyAction::SetNull),
        );
}

fn proposal_kind() -> Condition {
    Cond::any()
        .add(
            Cond::all()
                .add(Expr::col(SpaceProposal::Kind).eq("create"))
                .add(Expr::col(SpaceProposal::BaseVersion).is_null())
                .add(Expr::col(SpaceProposal::NewName).is_not_null())
                .add(Expr::col(SpaceProposal::NewSummary).is_not_null())
                .add(Expr::col(SpaceProposal::NewContent).is_not_null()),
        )
        .add(
            Cond::all()
                .add(Expr::col(SpaceProposal::Kind).eq("change"))
                .add(Expr::col(SpaceProposal::KnowledgeId).is_not_null())
                .add(Expr::col(SpaceProposal::BaseVersion).is_not_null())
                .add(
                    Cond::any()
                        .add(Expr::col(SpaceProposal::NewName).is_not_null())
                        .add(Expr::col(SpaceProposal::NewSummary).is_not_null())
                        .add(Expr::col(SpaceProposal::NewContent).is_not_null()),
                ),
        )
        .add(
            Cond::all()
                .add(Expr::col(SpaceProposal::Kind).eq("delete"))
                .add(Expr::col(SpaceProposal::KnowledgeId).is_not_null())
                .add(Expr::col(SpaceProposal::BaseVersion).is_not_null())
                .add(Expr::col(SpaceProposal::NewName).is_null())
                .add(Expr::col(SpaceProposal::NewSummary).is_null())
                .add(Expr::col(SpaceProposal::NewContent).is_null()),
        )
}

fn knowledge_version() -> TableCreateStatement {
    Table::create()
        .table(SpaceKnowledgeVersion::Table)
        .col(text(SpaceKnowledgeVersion::Id).primary_key())
        .col(text(SpaceKnowledgeVersion::KnowledgeId))
        .col(big_integer(SpaceKnowledgeVersion::Version))
        .col(text(SpaceKnowledgeVersion::Kind))
        .col(text_null(SpaceKnowledgeVersion::Content))
        .col(text_null(SpaceKnowledgeVersion::Summary))
        .col(text(SpaceKnowledgeVersion::AppliedBy))
        .col(text_null(SpaceKnowledgeVersion::ProposalId))
        .col(big_integer(SpaceKnowledgeVersion::CreatedAt))
        .index(
            Index::create()
                .unique()
                .col(SpaceKnowledgeVersion::KnowledgeId)
                .col(SpaceKnowledgeVersion::Version),
        )
        .check(Expr::col(SpaceKnowledgeVersion::Kind).is_in(["content", "deletion"]))
        .check(
            Cond::any()
                .add(
                    Cond::all()
                        .add(Expr::col(SpaceKnowledgeVersion::Kind).eq("content"))
                        .add(Expr::col(SpaceKnowledgeVersion::Content).is_not_null())
                        .add(Expr::col(SpaceKnowledgeVersion::Summary).is_not_null()),
                )
                .add(
                    Cond::all()
                        .add(Expr::col(SpaceKnowledgeVersion::Kind).eq("deletion"))
                        .add(Expr::col(SpaceKnowledgeVersion::Content).is_null())
                        .add(Expr::col(SpaceKnowledgeVersion::Summary).is_null()),
                ),
        )
        .foreign_key(
            ForeignKey::create()
                .from(
                    SpaceKnowledgeVersion::Table,
                    SpaceKnowledgeVersion::KnowledgeId,
                )
                .to(SpaceKnowledge::Table, SpaceKnowledge::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(
                    SpaceKnowledgeVersion::Table,
                    SpaceKnowledgeVersion::ProposalId,
                )
                .to(SpaceProposal::Table, SpaceProposal::Id),
        )
        .to_owned()
}

fn indexes() -> [IndexCreateStatement; 6] {
    [
        index(
            "space_task_space_activity",
            SpaceTask::Table,
            [SpaceTask::SpaceId, SpaceTask::LastActivityAt],
        ),
        index(
            "space_task_space_person",
            SpaceTask::Table,
            [SpaceTask::SpaceId, SpaceTask::PersonId, SpaceTask::Status],
        ),
        index(
            "space_note_task_time",
            SpaceNote::Table,
            [SpaceNote::TaskId, SpaceNote::CreatedAt],
        ),
        index(
            "space_artifact_task",
            SpaceArtifact::Table,
            [SpaceArtifact::TaskId],
        ),
        index(
            "space_proposal_space_status",
            SpaceProposal::Table,
            [
                SpaceProposal::SpaceId,
                SpaceProposal::Status,
                SpaceProposal::CreatedAt,
            ],
        ),
        index(
            "space_proposal_knowledge",
            SpaceProposal::Table,
            [SpaceProposal::KnowledgeId],
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
enum SpaceArtifact {
    CompletedAt,
    CreatedAt,
    FileName,
    Id,
    Number,
    RefusedAt,
    Size,
    SpaceId,
    StorageKey,
    Summary,
    Table,
    TaskId,
}

#[derive(DeriveIden)]
enum SpaceKnowledge {
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
enum SpaceKnowledgeVersion {
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
enum SpaceNote {
    Content,
    CreatedAt,
    Id,
    Table,
    TaskId,
}

#[derive(DeriveIden)]
enum SpaceProposal {
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
    ProposedBy,
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
    Number,
    OwnerId,
    Summary,
    Table,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum SpaceTask {
    AgentName,
    EndedAt,
    Id,
    LastActivityAt,
    Number,
    PersonId,
    RandomId,
    Result,
    SpaceId,
    StartedAt,
    Status,
    Table,
    Title,
}
