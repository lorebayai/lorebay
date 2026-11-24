use sea_orm::{DbErr, IntoActiveModel, Set};
use serde::Serialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::{space, space_agent_file as agent_file, space_task as task};
use crate::repository::{AgentFileRepository, SpaceRepository, TaskRepository};
use crate::{
    Caller, DownloadPresentation, DownloadRequest, DownloadSpec, LibraryState, OperationHandle,
    UploadRequest, UploadSpec, Write, WriteOperation,
};

use super::id::primary_key;
use super::key::{format_key, parse_key};
use super::space_access::{READ_ROLES, SpaceAccessError, check_space};
use super::task_access::{TaskAccessError, TaskAction, check_task};
use super::time::now_millis;
use super::{CallerError, NewAgentFile, TaskTarget};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AgentFileError {
    #[error(transparent)]
    Access(#[from] TaskAccessError),
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("File not found")]
    NotFound,
    #[error("The file content has not been uploaded")]
    NotUploaded,
    #[error("The file registration was refused; register a new file")]
    Refused,
    #[error("The file content exceeds the size limit")]
    TooLarge,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum AgentFileCompletion {
    Completed(AgentFileCompleted),
    Refused,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct AgentFile {
    pub key: String,
    pub agent_name: String,
    pub file_name: String,
    pub summary: String,
    pub size: i64,
    pub created_at: i64,
    pub completed_at: i64,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct AgentFileCompleted {
    #[serde(flatten)]
    pub agent_file: AgentFile,
    pub task_title: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct AgentFileRegistered {
    pub key: String,
    #[serde(flatten)]
    pub upload: UploadRequest,
    pub task_title: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(builder_method(vis = "pub(crate)"), field_defaults(setter(into)))]
pub struct AgentFileRegistration {
    key: String,
    location: String,
    task_title: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(builder_method(vis = "pub(crate)"), field_defaults(setter(into)))]
pub struct PreparedAgentFileCompletion {
    agent_file_id: String,
    key: String,
    size: Option<u64>,
    time: i64,
}

impl AgentFileRegistration {
    /// Issues the upload URL after the registration's write has committed.
    pub async fn issue_upload(
        self,
        state: &LibraryState,
    ) -> Result<AgentFileRegistered, AgentFileError> {
        let upload = state
            .storage
            .upload_url(UploadSpec {
                location: &self.location,
                lifetime: state.settings().upload_url_lifetime,
            })
            .await?;
        Ok(AgentFileRegistered {
            key: self.key,
            upload,
            task_title: self.task_title,
        })
    }
}

impl AgentFileCompletion {
    /// Converts a committed refusal into the caller's size-limit error.
    pub fn into_result(self) -> Result<AgentFileCompleted, AgentFileError> {
        match self {
            Self::Completed(agent_file) => Ok(agent_file),
            Self::Refused => Err(AgentFileError::TooLarge),
        }
    }
}

impl TryFrom<agent_file::Model> for AgentFile {
    type Error = DbErr;

    fn try_from(model: agent_file::Model) -> Result<Self, Self::Error> {
        Ok(Self::builder()
            .key(format_key("FL", model.number))
            .agent_name(model.agent_name)
            .file_name(model.file_name)
            .summary(model.summary)
            .size(
                model
                    .size
                    .ok_or_else(|| DbErr::Type("Incomplete file size".into()))?,
            )
            .created_at(model.created_at)
            .completed_at(
                model
                    .completed_at
                    .ok_or_else(|| DbErr::Type("Incomplete file time".into()))?,
            )
            .build())
    }
}

impl From<SpaceAccessError> for AgentFileError {
    fn from(error: SpaceAccessError) -> Self {
        Self::Access(error.into())
    }
}

impl AgentFileError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Access(error) => error.code(),
            Self::Caller(error) => error.code(),
            Self::Database(_) | Self::Internal(_) => "server.internal",
            Self::NotFound => "agent_file.not_found",
            Self::NotUploaded => "agent_file.not_uploaded",
            Self::Refused => "agent_file.refused",
            Self::TooLarge => "agent_file.too_large",
        }
    }
}

pub async fn check_agent_file_download(
    state: &LibraryState,
    location: &str,
) -> Result<DownloadPresentation, AgentFileError> {
    let file_name = state
        .run_read(async |handle| {
            Ok::<_, AgentFileError>(
                AgentFileRepository::builder()
                    .connection(handle.transaction())
                    .build()
                    .completed_by_storage_key(location)
                    .await?
                    .ok_or(AgentFileError::NotFound)?
                    .file_name,
            )
        })
        .await?;
    Ok(presentation(state, file_name))
}

pub async fn get_agent_file_download_url(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    key: &str,
) -> Result<DownloadRequest, AgentFileError> {
    let agent_file = state
        .run_read(async |handle| {
            let space = check_space(state, handle, caller, space, READ_ROLES).await?;
            let number = agent_file_number(key)?;
            AgentFileRepository::builder()
                .connection(handle.transaction())
                .build()
                .completed_by_space_number(&space.space.id, number)
                .await?
                .ok_or(AgentFileError::NotFound)
        })
        .await?;
    Ok(state
        .storage
        .download_url(DownloadSpec {
            location: &agent_file.storage_key,
            lifetime: state.settings().download_url_lifetime,
            presentation: presentation(state, agent_file.file_name),
        })
        .await?)
}

pub async fn register_agent_file(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    input: NewAgentFile,
) -> Result<AgentFileRegistration, AgentFileError> {
    caller.agent()?;
    let (number, location, task_title) = insert_registration(write, caller, target, input).await?;
    Ok(AgentFileRegistration::builder()
        .key(format_key("FL", number))
        .location(location)
        .task_title(task_title)
        .build())
}

/// Reads the file size before opening the completion's write transaction.
pub async fn prepare_agent_file_completion(
    state: &LibraryState,
    caller: &Caller,
    target: TaskTarget<'_>,
    key: &str,
) -> Result<PreparedAgentFileCompletion, AgentFileError> {
    caller.agent()?;
    let (_, agent_file) = state
        .run_read(async |handle| checked_agent_file(state, handle, caller, target, key).await)
        .await?;
    if agent_file.refused_at.is_some() {
        return Err(AgentFileError::Refused);
    }
    let size = if agent_file.completed_at.is_some() {
        None
    } else {
        Some(stored_size(state, &agent_file.storage_key).await?)
    };
    Ok(PreparedAgentFileCompletion::builder()
        .agent_file_id(agent_file.id)
        .key(key)
        .size(size)
        .time(now_millis()?)
        .build())
}

pub async fn delete_agent_file(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    key: &str,
) -> Result<(), AgentFileError> {
    write.begin_operation(Some(caller))?;
    caller.person()?;
    let task = check_task(
        write.state(),
        write.handle(),
        caller,
        target.check(TaskAction::DeleteAgentFile),
    )
    .await?;
    let number = agent_file_number(key)?;
    let location = AgentFileRepository::builder()
        .connection(write.transaction())
        .build()
        .delete_completed(&task.id, number)
        .await?
        .ok_or(AgentFileError::NotFound)?;
    write.delete.push(location);
    write.record(WriteOperation::DeleteAgentFile, Some(task.space_id));
    Ok(())
}

pub async fn complete_agent_file(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    completion: PreparedAgentFileCompletion,
) -> Result<AgentFileCompletion, AgentFileError> {
    write.begin_operation(Some(caller))?;
    caller.agent()?;
    let (task, current) = checked_agent_file(
        write.state(),
        write.handle(),
        caller,
        target,
        &completion.key,
    )
    .await?;
    if current.id != completion.agent_file_id {
        return Err(AgentFileError::NotFound);
    }
    if current.refused_at.is_some() {
        return Err(AgentFileError::Refused);
    }
    let space_id = task.space_id.clone();
    if current.completed_at.is_some() {
        let result = completed(current, task.title)?;
        write.finish_without_record();
        return Ok(AgentFileCompletion::Completed(result));
    }
    let size = completion.size.ok_or(AgentFileError::NotUploaded)?;
    if size > write.state().settings().upload_size_limit {
        return refuse_completion(write, current, space_id, completion.time).await;
    }
    let size = i64::try_from(size).map_err(anyhow::Error::from)?;
    let time = completion.time.max(task.last_activity_at);
    let transaction = write.transaction();
    let mut current = current.into_active_model();
    current.size = Set(Some(size));
    current.completed_at = Set(Some(time));
    let current = AgentFileRepository::builder()
        .connection(transaction)
        .build()
        .update(current)
        .await?;
    let title = task.title.clone();
    let mut task = task.into_active_model();
    task.work_recorded = Set(1);
    task.last_activity_at = Set(time);
    TaskRepository::builder()
        .connection(transaction)
        .build()
        .update(task)
        .await?;
    let result = completed(current, title)?;
    write.record(WriteOperation::CompleteAgentFile, Some(space_id));
    Ok(AgentFileCompletion::Completed(result))
}

async fn insert_registration(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    input: NewAgentFile,
) -> Result<(i64, String, String), AgentFileError> {
    write.begin_operation(Some(caller))?;
    let task = check_task(
        write.state(),
        write.handle(),
        caller,
        target.check(TaskAction::RegisterAgentFile),
    )
    .await?;
    let transaction = write.transaction();
    let title = task.title.clone();
    let space_id = task.space_id.clone();
    let task_id = task.id.clone();
    let mut model = task.into_active_model();
    model.work_recorded = Set(1);
    TaskRepository::builder()
        .connection(transaction)
        .build()
        .update(model)
        .await?;
    let number = SpaceRepository::builder()
        .connection(transaction)
        .build()
        .take_number(&space_id, space::Column::NextAgentFileNumber)
        .await?;
    let location = primary_key();
    AgentFileRepository::builder()
        .connection(transaction)
        .build()
        .insert(agent_file::ActiveModel {
            id: Set(primary_key()),
            space_id: Set(space_id.clone()),
            task_id: Set(task_id),
            agent_name: Set(caller.agent()?.to_owned()),
            number: Set(number),
            file_name: Set(input.file_name.into()),
            summary: Set(input.summary.into()),
            storage_key: Set(location.clone()),
            size: Set(None),
            created_at: Set(now_millis()?),
            completed_at: Set(None),
            refused_at: Set(None),
        })
        .await?;
    write.record(WriteOperation::RegisterAgentFile, Some(space_id));
    Ok((number, location, title))
}

async fn refuse_completion(
    write: &mut Write<'_>,
    current: agent_file::Model,
    space_id: String,
    time: i64,
) -> Result<AgentFileCompletion, AgentFileError> {
    let location = current.storage_key.clone();
    let mut current = current.into_active_model();
    current.refused_at = Set(Some(time));
    AgentFileRepository::builder()
        .connection(write.transaction())
        .build()
        .update(current)
        .await?;
    write.delete.push(location);
    write.record(WriteOperation::RefuseAgentFile, Some(space_id));
    Ok(AgentFileCompletion::Refused)
}

async fn stored_size(state: &LibraryState, location: &str) -> Result<u64, AgentFileError> {
    state
        .storage
        .size(location)
        .await?
        .ok_or(AgentFileError::NotUploaded)
}

async fn checked_agent_file(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    key: &str,
) -> Result<(task::Model, agent_file::Model), AgentFileError> {
    let task = check_task(
        state,
        handle,
        caller,
        target.check(TaskAction::CompleteAgentFile),
    )
    .await?;
    let number = agent_file_number(key)?;
    let agent_file = AgentFileRepository::builder()
        .connection(handle.transaction())
        .build()
        .by_number(
            &task.id,
            number,
            matches!(handle, OperationHandle::Write(_)),
        )
        .await?
        .ok_or(AgentFileError::NotFound)?;
    Ok((task, agent_file))
}

fn agent_file_number(key: &str) -> Result<i64, AgentFileError> {
    parse_key("FL", key).ok_or(AgentFileError::NotFound)
}

fn completed(
    agent_file: agent_file::Model,
    task_title: String,
) -> Result<AgentFileCompleted, AgentFileError> {
    Ok(AgentFileCompleted {
        agent_file: AgentFile::try_from(agent_file)?,
        task_title,
    })
}

fn presentation(state: &LibraryState, file_name: String) -> DownloadPresentation {
    DownloadPresentation::from_name(file_name, &state.settings().inline_content_types)
}
