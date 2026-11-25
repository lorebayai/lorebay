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
            task_result(),
            checkpoint(),
            agent_file(),
            knowledge(manager.get_database_backend()),
            person_request(),
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
            SpacePersonRequest::Table.into_iden(),
            SpaceKnowledge::Table.into_iden(),
            SpaceAgentFile::Table.into_iden(),
            SpaceCheckpoint::Table.into_iden(),
            SpaceTaskResult::Table.into_iden(),
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
        .col(text(Space::TenantId))
        .col(text(Space::Name))
        .col(text(Space::Summary))
        .col(big_integer(Space::NextTaskNumber).default(1))
        .col(big_integer(Space::NextKnowledgeNumber).default(1))
        .col(big_integer(Space::NextAgentFileNumber).default(1))
        .col(big_integer(Space::NextPersonRequestNumber).default(1))
        .col(big_integer(Space::CreatedAt))
        .col(big_integer(Space::UpdatedAt))
        .to_owned()
}

fn task() -> TableCreateStatement {
    let mut table = Table::create()
        .table(SpaceTask::Table)
        .col(text(SpaceTask::Id).primary_key())
        .col(text(SpaceTask::SpaceId))
        .col(big_integer(SpaceTask::Number))
        .col(text_uniq(SpaceTask::RandomId))
        .col(text(SpaceTask::PersonId))
        .col(text_null(SpaceTask::AgentName))
        .col(text_null(SpaceTask::Holder))
        .col(text_null(SpaceTask::PlannerPersonId))
        .col(text_null(SpaceTask::PlannerAgentName))
        .col(text_null(SpaceTask::StateReason))
        .col(text(SpaceTask::StateChangedBy))
        .col(text_null(SpaceTask::StateChangedAgentName))
        .col(big_integer(SpaceTask::StateChangedAt))
        .col(text_null(SpaceTask::WaitingRequestId))
        .col(big_integer(SpaceTask::WorkRecorded).default(0))
        .col(big_integer(SpaceTask::CreatedAt))
        .col(text(SpaceTask::Title))
        .col(text(SpaceTask::State))
        .col(text(SpaceTask::Result).default(""))
        .col(big_integer(SpaceTask::ResultVersion).default(0))
        .col(big_integer_null(SpaceTask::StartedAt))
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
        .foreign_key(
            ForeignKey::create()
                .from(SpaceTask::Table, SpaceTask::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .to_owned();
    task_checks(&mut table);
    table
}

fn task_checks(table: &mut TableCreateStatement) {
    table
        .check(Expr::col(SpaceTask::State).is_in([
            "planned", "working", "waiting", "done", "failed", "canceled",
        ]))
        .check(
            Expr::col(SpaceTask::State)
                .is_in(["working", "waiting"])
                .eq(Expr::col(SpaceTask::Holder).is_not_null()),
        )
        .check(
            Expr::col(SpaceTask::State)
                .is_in(["done", "failed", "canceled"])
                .eq(Expr::col(SpaceTask::EndedAt).is_not_null()),
        )
        .check(
            Expr::col(SpaceTask::State)
                .is_in(["waiting", "failed", "canceled"])
                .eq(Expr::col(SpaceTask::StateReason).is_not_null()),
        )
        .check(Expr::col(SpaceTask::WorkRecorded).is_in([0, 1]));
}

fn checkpoint() -> TableCreateStatement {
    Table::create()
        .table(SpaceCheckpoint::Table)
        .col(text(SpaceCheckpoint::Id).primary_key())
        .col(text(SpaceCheckpoint::TaskId))
        .col(text(SpaceCheckpoint::AgentName))
        .col(text(SpaceCheckpoint::Content))
        .col(big_integer(SpaceCheckpoint::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .from(SpaceCheckpoint::Table, SpaceCheckpoint::TaskId)
                .to(SpaceTask::Table, SpaceTask::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn task_result() -> TableCreateStatement {
    Table::create()
        .table(SpaceTaskResult::Table)
        .col(text(SpaceTaskResult::Id).primary_key())
        .col(text(SpaceTaskResult::TaskId))
        .col(big_integer(SpaceTaskResult::Version))
        .col(text(SpaceTaskResult::Result))
        .col(text(SpaceTaskResult::PersonId))
        .col(text(SpaceTaskResult::AgentName))
        .col(big_integer(SpaceTaskResult::CreatedAt))
        .index(
            Index::create()
                .unique()
                .col(SpaceTaskResult::TaskId)
                .col(SpaceTaskResult::Version),
        )
        .foreign_key(
            ForeignKey::create()
                .from(SpaceTaskResult::Table, SpaceTaskResult::TaskId)
                .to(SpaceTask::Table, SpaceTask::Id)
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}

fn agent_file() -> TableCreateStatement {
    Table::create()
        .table(SpaceAgentFile::Table)
        .col(text(SpaceAgentFile::Id).primary_key())
        .col(text(SpaceAgentFile::SpaceId))
        .col(text(SpaceAgentFile::TaskId))
        .col(text(SpaceAgentFile::AgentName))
        .col(big_integer(SpaceAgentFile::Number))
        .col(text(SpaceAgentFile::FileName))
        .col(text(SpaceAgentFile::Summary))
        .col(text_uniq(SpaceAgentFile::StorageKey))
        .col(big_integer_null(SpaceAgentFile::Size))
        .col(big_integer(SpaceAgentFile::CreatedAt))
        .col(big_integer_null(SpaceAgentFile::CompletedAt))
        .col(big_integer_null(SpaceAgentFile::RefusedAt))
        .index(
            Index::create()
                .unique()
                .col(SpaceAgentFile::SpaceId)
                .col(SpaceAgentFile::Number),
        )
        .check(
            Expr::col(SpaceAgentFile::CompletedAt)
                .is_null()
                .eq(Expr::col(SpaceAgentFile::Size).is_null()),
        )
        .check(
            Expr::col(SpaceAgentFile::RefusedAt)
                .is_null()
                .or(Expr::col(SpaceAgentFile::CompletedAt).is_null()),
        )
        .foreign_key(
            ForeignKey::create()
                .from(SpaceAgentFile::Table, SpaceAgentFile::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(
                    SpaceAgentFile::Table,
                    (SpaceAgentFile::TaskId, SpaceAgentFile::SpaceId),
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

fn person_request() -> TableCreateStatement {
    let mut table = Table::create()
        .table(SpacePersonRequest::Table)
        .col(text(SpacePersonRequest::Id).primary_key())
        .col(text(SpacePersonRequest::SpaceId))
        .col(big_integer(SpacePersonRequest::Number))
        .col(text(SpacePersonRequest::Action))
        .col(text_null(SpacePersonRequest::KnowledgeId))
        .col(big_integer_null(SpacePersonRequest::BaseVersion))
        .col(text_null(SpacePersonRequest::BaseName))
        .col(big_integer(SpacePersonRequest::Revision).default(1))
        .col(text_null(SpacePersonRequest::ReviserAgentName))
        .col(text_null(SpacePersonRequest::RevisedBy))
        .col(big_integer_null(SpacePersonRequest::RevisedAt))
        .col(text_null(SpacePersonRequest::NewName))
        .col(text_null(SpacePersonRequest::NewSummary))
        .col(text_null(SpacePersonRequest::NewContent))
        .col(text(SpacePersonRequest::RequestedBy))
        .col(text_null(SpacePersonRequest::RequesterAgentName))
        .col(text_null(SpacePersonRequest::FromTaskId))
        .col(text(SpacePersonRequest::Status))
        .col(text_null(SpacePersonRequest::DecidedBy))
        .col(big_integer_null(SpacePersonRequest::DecidedAt))
        .col(text_null(SpacePersonRequest::RejectReason))
        .col(big_integer(SpacePersonRequest::CreatedAt))
        .index(
            Index::create()
                .unique()
                .col(SpacePersonRequest::SpaceId)
                .col(SpacePersonRequest::Number),
        )
        .to_owned();
    person_request_checks(&mut table);
    person_request_foreign_keys(&mut table);
    table
}

fn person_request_checks(table: &mut TableCreateStatement) {
    table
        .check(Expr::col(SpacePersonRequest::Action).is_in(["create", "change", "delete"]))
        .check(Expr::col(SpacePersonRequest::Status).is_in(["pending", "approved", "rejected"]))
        .check(person_request_action())
        .check(Expr::col(SpacePersonRequest::Revision).gte(1))
        .check(
            Cond::any()
                .add(
                    Cond::all()
                        .add(Expr::col(SpacePersonRequest::Status).eq("pending"))
                        .add(Expr::col(SpacePersonRequest::DecidedBy).is_null())
                        .add(Expr::col(SpacePersonRequest::DecidedAt).is_null()),
                )
                .add(
                    Cond::all()
                        .add(Expr::col(SpacePersonRequest::Status).ne("pending"))
                        .add(Expr::col(SpacePersonRequest::DecidedBy).is_not_null())
                        .add(Expr::col(SpacePersonRequest::DecidedAt).is_not_null()),
                ),
        )
        .check(
            Cond::any()
                .add(Expr::col(SpacePersonRequest::Status).ne("rejected"))
                .add(Expr::col(SpacePersonRequest::RejectReason).is_not_null()),
        );
}

fn person_request_foreign_keys(table: &mut TableCreateStatement) {
    table
        .foreign_key(
            ForeignKey::create()
                .from(SpacePersonRequest::Table, SpacePersonRequest::SpaceId)
                .to(Space::Table, Space::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(SpacePersonRequest::Table, SpacePersonRequest::KnowledgeId)
                .to(SpaceKnowledge::Table, SpaceKnowledge::Id),
        )
        .foreign_key(
            ForeignKey::create()
                .from(SpacePersonRequest::Table, SpacePersonRequest::FromTaskId)
                .to(SpaceTask::Table, SpaceTask::Id)
                .on_delete(ForeignKeyAction::SetNull),
        );
}

fn person_request_action() -> Condition {
    Cond::any()
        .add(
            Cond::all()
                .add(Expr::col(SpacePersonRequest::Action).eq("create"))
                .add(Expr::col(SpacePersonRequest::BaseVersion).is_null())
                .add(Expr::col(SpacePersonRequest::BaseName).is_null())
                .add(Expr::col(SpacePersonRequest::NewName).is_not_null())
                .add(Expr::col(SpacePersonRequest::NewSummary).is_not_null())
                .add(Expr::col(SpacePersonRequest::NewContent).is_not_null()),
        )
        .add(
            Cond::all()
                .add(Expr::col(SpacePersonRequest::Action).eq("change"))
                .add(Expr::col(SpacePersonRequest::KnowledgeId).is_not_null())
                .add(Expr::col(SpacePersonRequest::BaseVersion).is_not_null())
                .add(Expr::col(SpacePersonRequest::BaseName).is_not_null())
                .add(
                    Cond::any()
                        .add(Expr::col(SpacePersonRequest::NewName).is_not_null())
                        .add(Expr::col(SpacePersonRequest::NewSummary).is_not_null())
                        .add(Expr::col(SpacePersonRequest::NewContent).is_not_null()),
                ),
        )
        .add(
            Cond::all()
                .add(Expr::col(SpacePersonRequest::Action).eq("delete"))
                .add(Expr::col(SpacePersonRequest::KnowledgeId).is_not_null())
                .add(Expr::col(SpacePersonRequest::BaseVersion).is_not_null())
                .add(Expr::col(SpacePersonRequest::BaseName).is_not_null())
                .add(Expr::col(SpacePersonRequest::NewName).is_null())
                .add(Expr::col(SpacePersonRequest::NewSummary).is_null())
                .add(Expr::col(SpacePersonRequest::NewContent).is_null()),
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
        .col(text_null(SpaceKnowledgeVersion::PersonRequestId))
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
                    SpaceKnowledgeVersion::PersonRequestId,
                )
                .to(SpacePersonRequest::Table, SpacePersonRequest::Id),
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
            [SpaceTask::SpaceId, SpaceTask::PersonId, SpaceTask::State],
        ),
        index(
            "space_checkpoint_task_time",
            SpaceCheckpoint::Table,
            [SpaceCheckpoint::TaskId, SpaceCheckpoint::CreatedAt],
        ),
        index(
            "space_agent_file_task",
            SpaceAgentFile::Table,
            [SpaceAgentFile::TaskId],
        ),
        index(
            "space_person_request_space_status",
            SpacePersonRequest::Table,
            [
                SpacePersonRequest::SpaceId,
                SpacePersonRequest::Status,
                SpacePersonRequest::CreatedAt,
            ],
        ),
        index(
            "space_person_request_knowledge",
            SpacePersonRequest::Table,
            [SpacePersonRequest::KnowledgeId],
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
enum SpaceAgentFile {
    AgentName,
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
    PersonRequestId,
    Summary,
    Table,
    Version,
}

#[derive(DeriveIden)]
enum SpaceCheckpoint {
    AgentName,
    Content,
    CreatedAt,
    Id,
    Table,
    TaskId,
}

#[derive(DeriveIden)]
enum SpacePersonRequest {
    Action,
    BaseName,
    BaseVersion,
    CreatedAt,
    DecidedAt,
    DecidedBy,
    FromTaskId,
    Id,
    KnowledgeId,
    NewContent,
    NewName,
    NewSummary,
    Number,
    RejectReason,
    RequestedBy,
    RequesterAgentName,
    RevisedAt,
    RevisedBy,
    ReviserAgentName,
    Revision,
    SpaceId,
    Status,
    Table,
}

#[derive(DeriveIden)]
enum Space {
    CreatedAt,
    Id,
    Name,
    NextAgentFileNumber,
    NextKnowledgeNumber,
    NextPersonRequestNumber,
    NextTaskNumber,
    Number,
    Summary,
    Table,
    TenantId,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum SpaceTask {
    AgentName,
    CreatedAt,
    EndedAt,
    Holder,
    Id,
    LastActivityAt,
    Number,
    PersonId,
    PlannerAgentName,
    PlannerPersonId,
    RandomId,
    Result,
    ResultVersion,
    SpaceId,
    StartedAt,
    State,
    StateChangedAgentName,
    StateChangedAt,
    StateChangedBy,
    StateReason,
    Table,
    Title,
    WaitingRequestId,
    WorkRecorded,
}

#[derive(DeriveIden)]
enum SpaceTaskResult {
    AgentName,
    CreatedAt,
    Id,
    PersonId,
    Result,
    Table,
    TaskId,
    Version,
}
