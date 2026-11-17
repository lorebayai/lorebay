use lorebay_base::Secret;
use sea_orm::{
    DatabaseConnection, DatabaseTransaction, DbErr, IntoActiveModel, Set, SqliteTransactionMode,
    TransactionOptions, TransactionTrait,
};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::{account, note, task};
use crate::repository::account::AccountRepository;
use crate::repository::{NoteRepository, SpaceRepository, TaskAfter, TaskRepository};

use super::caller::{CallerError, VerifiedCaller};
use super::login_session::now_millis;
use super::pagination::{Page, PageOptions, decode_cursor, encode_cursor};
use super::{
    NoteContent, SpaceAccessError, TASK_READ_ROLES, TASK_WRITE_ROLES, TaskAccessError, TaskAction,
    TaskResult, TaskTitle, check_space, check_task, format_key, parse_key, primary_key,
    random_task_id,
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
    Internal(#[from] anyhow::Error),
    #[error("Invalid cursor")]
    InvalidCursor,
    #[error("Task not found")]
    NotFound,
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
    TaskRepository::builder()
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
    let time = match repository.latest_time(&task.id).await? {
        Some(latest) => now_millis()?.max(
            latest
                .checked_add(1)
                .ok_or_else(|| DbErr::Type("Note times exhausted".into()))?,
        ),
        None => now_millis()?,
    };
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
) -> Result<Task, TaskError> {
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
        model.last_activity_at = Set(now);
    }
    let task = TaskRepository::builder()
        .connection(&transaction)
        .build()
        .update(model)
        .await?;
    let task = Task::try_from((task, account))?;
    transaction.commit().await?;
    Ok(task)
}

pub(crate) async fn list_tasks(
    database: &DatabaseConnection,
    caller: &VerifiedCaller,
    space: &str,
    options: PageOptions,
) -> Result<Page<Task>, TaskError> {
    let transaction = database.begin().await?;
    let authorized = check_space(&transaction, caller, space, TASK_READ_ROLES).await?;
    let version = format!("tasks-v1:{space}");
    let after = task_position(&options, &version, caller.account_id())?;
    let mut records = TaskRepository::builder()
        .connection(&transaction)
        .build()
        .list(&authorized.space.id, after.as_ref(), options.page_size + 1)
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
    let task = Task::try_from(record)?;
    transaction.commit().await?;
    Ok(TaskRead::builder().task(task).scratchpad(notes).build())
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

async fn write_transaction(database: &DatabaseConnection) -> Result<DatabaseTransaction, DbErr> {
    database
        .begin_with_options(TransactionOptions {
            sqlite_transaction_mode: Some(SqliteTransactionMode::Immediate),
            ..Default::default()
        })
        .await
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
