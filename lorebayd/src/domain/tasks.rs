use lorebay_base::Secret;
use sea_orm::{
    DatabaseConnection, DatabaseTransaction, DbErr, IntoActiveModel, Set, SqliteTransactionMode,
    TransactionOptions, TransactionTrait,
};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::{account, note, task};
use crate::repository::account::AccountRepository;
use crate::repository::{
    ArtifactRepository, NoteRepository, SpaceRepository, TaskAfter, TaskListFilter, TaskRepository,
};

use super::caller::{CallerError, VerifiedCaller};
use super::login_session::now_millis;
use super::pagination::{Page, PageOptions, decode_cursor, encode_cursor};
use super::{
    Artifact, NoteContent, SpaceAccessError, TASK_READ_ROLES, TASK_WRITE_ROLES, TaskAccessError,
    TaskAction, TaskFilterError, TaskFilters, TaskKeyword, TaskResult, TaskTitle, check_space,
    check_task, format_key, parse_key, primary_key, random_task_id,
};

#[derive(Debug, Error)]
pub(crate) enum TaskError {
    #[error(transparent)]
    Access(#[from] TaskAccessError),
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Filter(#[from] TaskFilterError),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error("Task not found")]
    NotFound,
    #[error("Note not found")]
    NoteNotFound,
    #[error(transparent)]
    Space(#[from] SpaceAccessError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskStatus {
    Ended,
    InProgress,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct CreatedTask {
    pub task_title: String,
    pub key: String,
    pub task_id: Secret<String>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Note {
    pub time: i64,
    pub content: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct NoteAdded {
    pub time: i64,
    pub task_title: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Task {
    pub key: String,
    pub task_id: Secret<String>,
    pub title: String,
    pub status: TaskStatus,
    pub person: String,
    pub agent_name: String,
    pub result: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub last_activity_at: i64,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskChanges {
    pub title: Option<TaskTitle>,
    pub status: Option<TaskStatus>,
    pub result: Option<TaskResult>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct TaskRead {
    pub task: Task,
    pub scratchpad: Option<Page<Note>>,
    pub artifacts: Vec<Artifact>,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ended => "ended",
            Self::InProgress => "in progress",
        }
    }
}

impl TryFrom<&str> for TaskStatus {
    type Error = DbErr;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "ended" => Ok(Self::Ended),
            "in progress" => Ok(Self::InProgress),
            _ => Err(DbErr::Type("Invalid task status".into())),
        }
    }
}

impl TryFrom<(task::Model, Option<account::Model>)> for Task {
    type Error = DbErr;

    fn try_from(
        (task, account): (task::Model, Option<account::Model>),
    ) -> Result<Self, Self::Error> {
        let account = account.ok_or_else(|| DbErr::Type("Missing task account".into()))?;
        Ok(Self::builder()
            .key(format_key("TK", task.number))
            .task_id(Secret::from(task.random_id))
            .title(task.title)
            .status(TaskStatus::try_from(task.status.as_str())?)
            .person(account.username)
            .agent_name(task.agent_name)
            .result(task.result)
            .started_at(task.started_at)
            .ended_at(task.ended_at)
            .last_activity_at(task.last_activity_at)
            .build())
    }
}

impl From<note::Model> for Note {
    fn from(note: note::Model) -> Self {
        Self::builder()
            .time(note.created_at)
            .content(note.content)
            .build()
    }
}

pub(crate) async fn create_task(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    space: &str,
    title: TaskTitle,
) -> Result<CreatedTask, TaskError> {
    let agent = caller.agent()?;
    let transaction = write_transaction(database).await?;
    let authorized = check_space(&transaction, caller, space, TASK_WRITE_ROLES).await?;
    let number = authorized.space.next_task_number;
    let next = number
        .checked_add(1)
        .ok_or_else(|| DbErr::Type("Task numbers exhausted".into()))?;
    let mut space = authorized.space.into_active_model();
    space.next_task_number = Set(next);
    let space = SpaceRepository::builder()
        .connection(&transaction)
        .build()
        .update(space)
        .await?;
    let now = now_millis()?;
    let task_id = random_task_id().map_err(anyhow::Error::from)?;
    let task = TaskRepository::builder()
        .connection(&transaction)
        .build()
        .insert(task::ActiveModel {
            id: Set(primary_key()),
            space_id: Set(space.id),
            number: Set(number),
            random_id: Set(task_id.clone()),
            account_id: Set(agent.account_id.clone()),
            agent_name: Set(agent.name.clone()),
            title: Set(title.into()),
            status: Set(TaskStatus::InProgress.as_str().to_owned()),
            result: Set(String::new()),
            started_at: Set(now),
            ended_at: Set(None),
            last_activity_at: Set(now),
        })
        .await?;
    transaction.commit().await?;
    Ok(CreatedTask::builder()
        .task_title(task.title)
        .key(format_key("TK", number))
        .task_id(Secret::from(task_id))
        .build())
}

pub(crate) async fn add_note(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    path_space: Option<&str>,
    task_id: &str,
    content: NoteContent,
) -> Result<NoteAdded, TaskError> {
    caller.agent()?;
    let transaction = write_transaction(database).await?;
    let task = check_task(
        &transaction,
        caller,
        task_id,
        path_space,
        TaskAction::AppendNote,
    )
    .await?;
    let repository = NoteRepository::builder().connection(&transaction).build();
    // Deleted notes must not free their times for later appends.
    let latest = repository
        .latest_time(&task.id)
        .await?
        .unwrap_or(task.last_activity_at)
        .max(task.last_activity_at);
    let time = now_millis()?.max(
        latest
            .checked_add(1)
            .ok_or_else(|| DbErr::Type("Note times exhausted".into()))?,
    );
    repository
        .insert(note::ActiveModel {
            id: Set(primary_key()),
            task_id: Set(task.id.clone()),
            content: Set(content.into()),
            created_at: Set(time),
        })
        .await?;
    let title = task.title.clone();
    let mut task = task.into_active_model();
    task.last_activity_at = Set(time);
    TaskRepository::builder()
        .connection(&transaction)
        .build()
        .update(task)
        .await?;
    transaction.commit().await?;
    Ok(NoteAdded::builder().time(time).task_title(title).build())
}

pub(crate) async fn change_task(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    path_space: Option<&str>,
    task_id: &str,
    changes: TaskChanges,
) -> Result<TaskRead, TaskError> {
    let transaction = write_transaction(database).await?;
    let action = if changes.title.is_none()
        && changes.result.is_none()
        && changes.status == Some(TaskStatus::Ended)
    {
        TaskAction::Close
    } else {
        TaskAction::Change
    };
    let task = check_task(&transaction, caller, task_id, path_space, action).await?;
    let account = AccountRepository::builder()
        .connection(&transaction)
        .build()
        .by_id(&task.account_id)
        .await?;
    let status = changes
        .status
        .filter(|status| status.as_str() != task.status);
    let last_activity_at = task.last_activity_at;
    let mut model = task.into_active_model();
    if let Some(title) = changes.title {
        model.title = Set(title.into());
    }
    let now = now_millis()?;
    if let Some(status) = status {
        model.status = Set(status.as_str().to_owned());
        model.ended_at = Set(match status {
            TaskStatus::Ended => Some(now),
            TaskStatus::InProgress => None,
        });
    }
    if let Some(result) = changes.result {
        model.result = Set(result.into());
        model.last_activity_at = Set(now.max(last_activity_at));
    }
    let task = TaskRepository::builder()
        .connection(&transaction)
        .build()
        .update(model)
        .await?;
    let artifacts = completed_artifacts(&transaction, &task.id).await?;
    let task = Task::try_from((task, account))?;
    transaction.commit().await?;
    Ok(TaskRead::builder()
        .task(task)
        .artifacts(artifacts)
        .scratchpad(None)
        .build())
}

pub(crate) async fn delete_task(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    space: &str,
    task_id: &str,
) -> Result<(), TaskError> {
    caller.person()?;
    let transaction = write_transaction(database).await?;
    let task = check_task(
        &transaction,
        caller,
        task_id,
        Some(space),
        TaskAction::Delete,
    )
    .await?;
    TaskRepository::builder()
        .connection(&transaction)
        .build()
        .delete(&task.id)
        .await?;
    transaction.commit().await?;
    Ok(())
}

pub(crate) async fn delete_note(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    space: &str,
    task_id: &str,
    time: i64,
) -> Result<(), TaskError> {
    caller.person()?;
    let transaction = write_transaction(database).await?;
    let task = check_task(
        &transaction,
        caller,
        task_id,
        Some(space),
        TaskAction::Delete,
    )
    .await?;
    let deleted = NoteRepository::builder()
        .connection(&transaction)
        .build()
        .delete_by_time(&task.id, time)
        .await?;
    if deleted == 0 {
        return Err(TaskError::NoteNotFound);
    }
    transaction.commit().await?;
    Ok(())
}

pub(crate) async fn list_tasks(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    space: &str,
    options: PageOptions,
    filters: TaskFilters,
) -> Result<Page<Task>, TaskError> {
    filters.validate()?;
    let transaction = database.begin().await?;
    let authorized = check_space(&transaction, caller, space, TASK_READ_ROLES).await?;
    let version = filters.cursor_version(space).map_err(anyhow::Error::from)?;
    let after = task_position(&options, &version, caller.account_id())?;
    let filter = TaskListFilter::builder()
        .person(filters.person.as_deref())
        .account_id(filters.own_open.then(|| caller.account_id()))
        .agent(filters.agent.as_deref())
        .status(
            filters
                .own_open
                .then_some(TaskStatus::InProgress)
                .or(filters.status)
                .map(TaskStatus::as_str),
        )
        .keyword(filters.keyword.as_ref().map(TaskKeyword::as_str))
        .build();
    let mut records = TaskRepository::builder()
        .connection(&transaction)
        .build()
        .list(
            &authorized.space.id,
            &filter,
            after.as_ref(),
            options.page_size + 1,
        )
        .await?;
    let has_more = records.len() as u64 > options.page_size;
    records.truncate(options.page_size as usize);
    let next_cursor = records.last().filter(|_| has_more).map(|(task, _)| {
        encode_cursor(
            &version,
            caller.account_id(),
            &format!("{}:{}", task.last_activity_at, task.number),
        )
    });
    let items = records
        .into_iter()
        .map(Task::try_from)
        .collect::<Result<Vec<_>, _>>()?;
    transaction.commit().await?;
    Ok(Page::builder()
        .items(items)
        .next_cursor(next_cursor)
        .build())
}

pub(crate) async fn read_task(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    space: &str,
    key: &str,
    scratchpad: Option<PageOptions>,
) -> Result<TaskRead, TaskError> {
    let transaction = database.begin().await?;
    let record = task_by_key(&transaction, caller, space, key).await?;
    let notes = match scratchpad {
        Some(options) => Some(note_page(&transaction, caller, &record.0, space, options).await?),
        None => None,
    };
    let artifacts = completed_artifacts(&transaction, &record.0.id).await?;
    let task = Task::try_from(record)?;
    transaction.commit().await?;
    Ok(TaskRead::builder()
        .task(task)
        .scratchpad(notes)
        .artifacts(artifacts)
        .build())
}

pub(crate) async fn read_scratchpad(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    space: &str,
    key: &str,
    options: PageOptions,
) -> Result<Page<Note>, TaskError> {
    let transaction = database.begin().await?;
    let (task, _) = task_by_key(&transaction, caller, space, key).await?;
    let page = note_page(&transaction, caller, &task, space, options).await?;
    transaction.commit().await?;
    Ok(page)
}

pub(crate) async fn write_transaction(
    database: &DatabaseConnection,
) -> Result<DatabaseTransaction, DbErr> {
    database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
}

async fn completed_artifacts(
    transaction: &DatabaseTransaction,
    task_id: &str,
) -> Result<Vec<Artifact>, DbErr> {
    ArtifactRepository::builder()
        .connection(transaction)
        .build()
        .completed(task_id)
        .await?
        .into_iter()
        .map(Artifact::try_from)
        .collect()
}

async fn task_by_key(
    transaction: &DatabaseTransaction,
    caller: &VerifiedCaller,
    space: &str,
    key: &str,
) -> Result<(task::Model, Option<account::Model>), TaskError> {
    let authorized = check_space(transaction, caller, space, TASK_READ_ROLES).await?;
    let number = parse_key("TK", key).ok_or(TaskError::NotFound)?;
    TaskRepository::builder()
        .connection(transaction)
        .build()
        .by_number(&authorized.space.id, number)
        .await?
        .ok_or(TaskError::NotFound)
}

async fn note_page(
    transaction: &DatabaseTransaction,
    caller: &VerifiedCaller,
    task: &task::Model,
    space: &str,
    options: PageOptions,
) -> Result<Page<Note>, TaskError> {
    let version = format!("notes-v1:{space}:{}", task.number);
    let after = options
        .cursor
        .map(|cursor| {
            decode_cursor(&version, caller.account_id(), &cursor)
                .and_then(|value| value.parse::<i64>().ok())
                .filter(|time| *time >= 0)
                .ok_or(TaskError::InvalidCursor)
        })
        .transpose()?;
    let mut records = NoteRepository::builder()
        .connection(transaction)
        .build()
        .list(&task.id, after, options.page_size + 1)
        .await?;
    let has_more = records.len() as u64 > options.page_size;
    records.truncate(options.page_size as usize);
    let next_cursor = records
        .last()
        .filter(|_| has_more)
        .map(|note| encode_cursor(&version, caller.account_id(), &note.created_at.to_string()));
    Ok(Page::builder()
        .items(records.into_iter().map(Note::from).collect::<Vec<_>>())
        .next_cursor(next_cursor)
        .build())
}

fn task_position(
    options: &PageOptions,
    version: &str,
    account: &str,
) -> Result<Option<TaskAfter>, TaskError> {
    options
        .cursor
        .as_ref()
        .map(|cursor| {
            let value = decode_cursor(version, account, cursor).ok_or(TaskError::InvalidCursor)?;
            let (time, number) = value.split_once(':').ok_or(TaskError::InvalidCursor)?;
            let time = time.parse::<i64>().map_err(|_| TaskError::InvalidCursor)?;
            let number = number
                .parse::<i64>()
                .map_err(|_| TaskError::InvalidCursor)?;
            if time < 0 || number <= 0 {
                return Err(TaskError::InvalidCursor);
            }
            Ok(TaskAfter::builder()
                .last_activity_at(time)
                .number(number)
                .build())
        })
        .transpose()
}
