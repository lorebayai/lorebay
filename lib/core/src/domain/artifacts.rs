use sea_orm::{DbErr, IntoActiveModel, Set};
use serde::Serialize;
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::{space, space_artifact as artifact, space_task as task};
use crate::repository::{ArtifactRepository, SpaceRepository, TaskRepository};
use crate::{
    Caller, DownloadPresentation, DownloadRequest, DownloadSpec, LibraryState, OperationHandle,
    UploadRequest, UploadSpec, Write, WriteOperation,
};

use super::id::primary_key;
use super::key::{format_key, parse_key};
use super::space_access::{READ_ROLES, SpaceAccessError, check_space};
use super::task_access::{TaskAccessError, TaskAction, check_task};
use super::time::now_millis;
use super::{CallerError, NewArtifact, TaskTarget};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ArtifactError {
    #[error(transparent)]
    Access(#[from] TaskAccessError),
    #[error(transparent)]
    Caller(#[from] CallerError),
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Artifact not found")]
    NotFound,
    #[error("The artifact file has not been uploaded")]
    NotUploaded,
    #[error("The artifact registration was refused; register a new artifact")]
    Refused,
    #[error("The artifact file exceeds the size limit")]
    TooLarge,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ArtifactCompletion {
    Completed(ArtifactCompleted),
    Refused,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct Artifact {
    pub key: String,
    pub file_name: String,
    pub summary: String,
    pub size: i64,
    pub created_at: i64,
    pub completed_at: i64,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct ArtifactCompleted {
    #[serde(flatten)]
    pub artifact: Artifact,
    pub task_title: String,
}

#[derive(Clone, Debug, Serialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct ArtifactRegistered {
    pub key: String,
    #[serde(flatten)]
    pub upload: UploadRequest,
    pub task_title: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(builder_method(vis = "pub(crate)"), field_defaults(setter(into)))]
pub struct ArtifactRegistration {
    key: String,
    location: String,
    task_title: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(builder_method(vis = "pub(crate)"), field_defaults(setter(into)))]
pub struct PreparedArtifactCompletion {
    artifact_id: String,
    key: String,
    size: Option<u64>,
    time: i64,
}

impl ArtifactRegistration {
    /// Issues the upload URL after the registration's write has committed.
    pub async fn issue_upload(
        self,
        state: &LibraryState,
    ) -> Result<ArtifactRegistered, ArtifactError> {
        let upload = state
            .storage
            .upload_url(UploadSpec {
                location: &self.location,
                lifetime: state.settings().upload_url_lifetime,
            })
            .await?;
        Ok(ArtifactRegistered {
            key: self.key,
            upload,
            task_title: self.task_title,
        })
    }
}

impl ArtifactCompletion {
    /// Converts a committed refusal into the caller's size-limit error.
    pub fn into_result(self) -> Result<ArtifactCompleted, ArtifactError> {
        match self {
            Self::Completed(artifact) => Ok(artifact),
            Self::Refused => Err(ArtifactError::TooLarge),
        }
    }
}

impl TryFrom<artifact::Model> for Artifact {
    type Error = DbErr;

    fn try_from(model: artifact::Model) -> Result<Self, Self::Error> {
        Ok(Self::builder()
            .key(format_key("AR", model.number))
            .file_name(model.file_name)
            .summary(model.summary)
            .size(
                model
                    .size
                    .ok_or_else(|| DbErr::Type("Incomplete artifact size".into()))?,
            )
            .created_at(model.created_at)
            .completed_at(
                model
                    .completed_at
                    .ok_or_else(|| DbErr::Type("Incomplete artifact time".into()))?,
            )
            .build())
    }
}

impl From<SpaceAccessError> for ArtifactError {
    fn from(error: SpaceAccessError) -> Self {
        Self::Access(error.into())
    }
}

impl ArtifactError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Access(error) => error.code(),
            Self::Caller(error) => error.code(),
            Self::Database(_) | Self::Internal(_) => "server.internal",
            Self::NotFound => "artifact.not_found",
            Self::NotUploaded => "artifact.not_uploaded",
            Self::Refused => "artifact.refused",
            Self::TooLarge => "artifact.too_large",
        }
    }
}

pub async fn check_artifact_download(
    state: &LibraryState,
    location: &str,
) -> Result<DownloadPresentation, ArtifactError> {
    let file_name = state
        .run_read(async |handle| {
            Ok::<_, ArtifactError>(
                ArtifactRepository::builder()
                    .connection(handle.transaction())
                    .build()
                    .completed_by_storage_key(location)
                    .await?
                    .ok_or(ArtifactError::NotFound)?
                    .file_name,
            )
        })
        .await?;
    Ok(presentation(state, file_name))
}

pub async fn get_artifact_download_url(
    state: &LibraryState,
    caller: &Caller,
    space: &str,
    key: &str,
) -> Result<DownloadRequest, ArtifactError> {
    let artifact = state
        .run_read(async |handle| {
            let space = check_space(state, handle, caller, space, READ_ROLES).await?;
            let number = artifact_number(key)?;
            ArtifactRepository::builder()
                .connection(handle.transaction())
                .build()
                .completed_by_space_number(&space.space.id, number)
                .await?
                .ok_or(ArtifactError::NotFound)
        })
        .await?;
    Ok(state
        .storage
        .download_url(DownloadSpec {
            location: &artifact.storage_key,
            lifetime: state.settings().download_url_lifetime,
            presentation: presentation(state, artifact.file_name),
        })
        .await?)
}

pub async fn register_artifact(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    input: NewArtifact,
) -> Result<ArtifactRegistration, ArtifactError> {
    caller.agent()?;
    let (number, location, task_title) = insert_registration(write, caller, target, input).await?;
    Ok(ArtifactRegistration::builder()
        .key(format_key("AR", number))
        .location(location)
        .task_title(task_title)
        .build())
}

/// Reads the file size before opening the completion's write transaction.
pub async fn prepare_artifact_completion(
    state: &LibraryState,
    caller: &Caller,
    target: TaskTarget<'_>,
    key: &str,
) -> Result<PreparedArtifactCompletion, ArtifactError> {
    caller.agent()?;
    let (_, artifact) = state
        .run_read(async |handle| checked_artifact(state, handle, caller, target, key).await)
        .await?;
    if artifact.refused_at.is_some() {
        return Err(ArtifactError::Refused);
    }
    let size = if artifact.completed_at.is_some() {
        None
    } else {
        Some(stored_size(state, &artifact.storage_key).await?)
    };
    Ok(PreparedArtifactCompletion::builder()
        .artifact_id(artifact.id)
        .key(key)
        .size(size)
        .time(now_millis()?)
        .build())
}

pub async fn delete_artifact(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    key: &str,
) -> Result<(), ArtifactError> {
    write.begin_operation(Some(caller))?;
    caller.person()?;
    let task = check_task(
        write.state(),
        write.handle(),
        caller,
        target.check(TaskAction::DeleteArtifact),
    )
    .await?;
    let number = artifact_number(key)?;
    let location = ArtifactRepository::builder()
        .connection(write.transaction())
        .build()
        .delete_completed(&task.id, number)
        .await?
        .ok_or(ArtifactError::NotFound)?;
    write.delete.push(location);
    write.record(WriteOperation::DeleteArtifact, Some(task.space_id));
    Ok(())
}

pub async fn complete_artifact(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    completion: PreparedArtifactCompletion,
) -> Result<ArtifactCompletion, ArtifactError> {
    write.begin_operation(Some(caller))?;
    caller.agent()?;
    let (task, current) = checked_artifact(
        write.state(),
        write.handle(),
        caller,
        target,
        &completion.key,
    )
    .await?;
    if current.id != completion.artifact_id {
        return Err(ArtifactError::NotFound);
    }
    if current.refused_at.is_some() {
        return Err(ArtifactError::Refused);
    }
    let space_id = task.space_id.clone();
    if current.completed_at.is_some() {
        let result = completed(current, task.title)?;
        write.finish_without_record();
        return Ok(ArtifactCompletion::Completed(result));
    }
    let size = completion.size.ok_or(ArtifactError::NotUploaded)?;
    if size > write.state().settings().upload_size_limit {
        return refuse_completion(write, current, space_id, completion.time).await;
    }
    let size = i64::try_from(size).map_err(anyhow::Error::from)?;
    let time = completion.time.max(task.last_activity_at);
    let transaction = write.transaction();
    let mut current = current.into_active_model();
    current.size = Set(Some(size));
    current.completed_at = Set(Some(time));
    let current = ArtifactRepository::builder()
        .connection(transaction)
        .build()
        .update(current)
        .await?;
    let title = task.title.clone();
    let mut task = task.into_active_model();
    task.last_activity_at = Set(time);
    TaskRepository::builder()
        .connection(transaction)
        .build()
        .update(task)
        .await?;
    let result = completed(current, title)?;
    write.record(WriteOperation::CompleteArtifact, Some(space_id));
    Ok(ArtifactCompletion::Completed(result))
}

async fn insert_registration(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    input: NewArtifact,
) -> Result<(i64, String, String), ArtifactError> {
    write.begin_operation(Some(caller))?;
    let task = check_task(
        write.state(),
        write.handle(),
        caller,
        target.check(TaskAction::RegisterArtifact),
    )
    .await?;
    let transaction = write.transaction();
    let space_id = task.space_id;
    let number = SpaceRepository::builder()
        .connection(transaction)
        .build()
        .take_number(&space_id, space::Column::NextArtifactNumber)
        .await?;
    let location = primary_key();
    ArtifactRepository::builder()
        .connection(transaction)
        .build()
        .insert(artifact::ActiveModel {
            id: Set(primary_key()),
            space_id: Set(space_id.clone()),
            task_id: Set(task.id),
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
    write.record(WriteOperation::RegisterArtifact, Some(space_id));
    Ok((number, location, task.title))
}

async fn refuse_completion(
    write: &mut Write<'_>,
    current: artifact::Model,
    space_id: String,
    time: i64,
) -> Result<ArtifactCompletion, ArtifactError> {
    let location = current.storage_key.clone();
    let mut current = current.into_active_model();
    current.refused_at = Set(Some(time));
    ArtifactRepository::builder()
        .connection(write.transaction())
        .build()
        .update(current)
        .await?;
    write.delete.push(location);
    write.record(WriteOperation::RefuseArtifact, Some(space_id));
    Ok(ArtifactCompletion::Refused)
}

async fn stored_size(state: &LibraryState, location: &str) -> Result<u64, ArtifactError> {
    state
        .storage
        .size(location)
        .await?
        .ok_or(ArtifactError::NotUploaded)
}

async fn checked_artifact(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
    key: &str,
) -> Result<(task::Model, artifact::Model), ArtifactError> {
    let task = check_task(
        state,
        handle,
        caller,
        target.check(TaskAction::CompleteArtifact),
    )
    .await?;
    let number = artifact_number(key)?;
    let artifact = ArtifactRepository::builder()
        .connection(handle.transaction())
        .build()
        .by_number(
            &task.id,
            number,
            matches!(handle, OperationHandle::Write(_)),
        )
        .await?
        .ok_or(ArtifactError::NotFound)?;
    Ok((task, artifact))
}

fn artifact_number(key: &str) -> Result<i64, ArtifactError> {
    parse_key("AR", key).ok_or(ArtifactError::NotFound)
}

fn completed(
    artifact: artifact::Model,
    task_title: String,
) -> Result<ArtifactCompleted, ArtifactError> {
    Ok(ArtifactCompleted {
        artifact: Artifact::try_from(artifact)?,
        task_title,
    })
}

fn presentation(state: &LibraryState, file_name: String) -> DownloadPresentation {
    DownloadPresentation::from_name(file_name, &state.settings().inline_content_types)
}
