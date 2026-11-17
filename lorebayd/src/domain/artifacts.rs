use lorebay_base::Secret;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, IntoActiveModel, Set, TransactionTrait};
use thiserror::Error;
use typed_builder::TypedBuilder;

use crate::entity::{artifact, task};
use crate::repository::{ArtifactRepository, SpaceRepository, TaskRepository};
use crate::storage::{DownloadRequest, FileStorage, UploadRequest};

use super::caller::{CallerError, VerifiedCaller};
use super::login_session::now_millis;
use super::{
    NewArtifact, SpaceAccessError, TASK_READ_ROLES, TaskAccessError, TaskAction, check_space,
    check_task, format_key, parse_key, primary_key, write_transaction,
};

#[derive(Debug, Error)]
pub(crate) enum ArtifactError {
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
    #[error(transparent)]
    Space(#[from] SpaceAccessError),
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Artifact {
    pub key: String,
    pub file_name: String,
    pub summary: String,
    pub size: i64,
    pub created_at: i64,
    pub completed_at: i64,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ArtifactCompleted {
    pub artifact: Artifact,
    pub task_title: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ArtifactRegistered {
    pub key: String,
    pub upload: UploadRequest,
    pub task_title: String,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct ArtifactTarget<'a> {
    pub path_space: Option<&'a str>,
    pub task_id: &'a Secret<String>,
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

pub(crate) async fn get_artifact_download_url(
    database: &DatabaseConnection,
    storage: &impl FileStorage,
    caller: &VerifiedCaller,
    space: &str,
    key: &str,
) -> Result<DownloadRequest, ArtifactError> {
    let transaction = database.begin().await?;
    let space = check_space(&transaction, caller, space, TASK_READ_ROLES).await?;
    let number = parse_key("AR", key).ok_or(ArtifactError::NotFound)?;
    let artifact = ArtifactRepository::builder()
        .connection(&transaction)
        .build()
        .completed_by_space_number(&space.space.id, number)
        .await?
        .ok_or(ArtifactError::NotFound)?;
    transaction.commit().await?;
    Ok(storage
        .download_url(&artifact.storage_key, &artifact.file_name)
        .await?)
}

pub(crate) async fn register_artifact(
    database: &DatabaseConnection,
    storage: &impl FileStorage,
    caller: &VerifiedCaller,
    target: ArtifactTarget<'_>,
    input: NewArtifact,
) -> Result<ArtifactRegistered, ArtifactError> {
    caller.agent()?;
    let transaction = write_transaction(database).await?;
    let task = check_task(
        &transaction,
        caller,
        target.task_id.expose(),
        target.path_space,
        TaskAction::RegisterArtifact,
    )
    .await?;
    let space_repository = SpaceRepository::builder().connection(&transaction).build();
    let space = space_repository
        .by_id(&task.space_id)
        .await?
        .ok_or_else(|| DbErr::Type("Missing artifact space".into()))?;
    let number = space.next_artifact_number;
    let mut space = space.into_active_model();
    space.next_artifact_number = Set(number
        .checked_add(1)
        .ok_or_else(|| DbErr::Type("Artifact numbers exhausted".into()))?);
    space_repository.update(space).await?;
    let location = primary_key();
    ArtifactRepository::builder()
        .connection(&transaction)
        .build()
        .insert(artifact::ActiveModel {
            id: Set(primary_key()),
            space_id: Set(task.space_id),
            task_id: Set(task.id),
            number: Set(number),
            file_name: Set(input.file_name.into()),
            summary: Set(input.summary.into()),
            storage_key: Set(location.clone()),
            size: Set(None),
            created_at: Set(now_millis()?),
            completed_at: Set(None),
        })
        .await?;
    transaction.commit().await?;
    let upload = storage.upload_url(&location).await?;
    Ok(ArtifactRegistered::builder()
        .key(format_key("AR", number))
        .upload(upload)
        .task_title(task.title)
        .build())
}

pub(crate) async fn complete_artifact(
    database: &DatabaseConnection,
    storage: &impl FileStorage,
    caller: &VerifiedCaller,
    target: ArtifactTarget<'_>,
    key: &str,
) -> Result<ArtifactCompleted, ArtifactError> {
    caller.agent()?;
    let read = database.begin().await?;
    let (task, artifact) = checked_artifact(&read, caller, &target, key).await?;
    read.commit().await?;
    if artifact.completed_at.is_some() {
        return completed(artifact, task.title);
    }
    let size = storage
        .size(&artifact.storage_key)
        .await?
        .ok_or(ArtifactError::NotUploaded)?;
    let transaction = write_transaction(database).await?;
    let (task, current) = checked_artifact(&transaction, caller, &target, key).await?;
    if current.completed_at.is_some() {
        transaction.commit().await?;
        return completed(current, task.title);
    }
    let time = now_millis()?.max(task.last_activity_at);
    let mut current = current.into_active_model();
    current.size = Set(Some(i64::try_from(size).map_err(anyhow::Error::from)?));
    current.completed_at = Set(Some(time));
    let current = ArtifactRepository::builder()
        .connection(&transaction)
        .build()
        .update(current)
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
    completed(current, title)
}

async fn checked_artifact<C: ConnectionTrait>(
    connection: &C,
    caller: &VerifiedCaller,
    target: &ArtifactTarget<'_>,
    key: &str,
) -> Result<(task::Model, artifact::Model), ArtifactError> {
    let task = check_task(
        connection,
        caller,
        target.task_id.expose(),
        target.path_space,
        TaskAction::CompleteArtifact,
    )
    .await?;
    let number = parse_key("AR", key).ok_or(ArtifactError::NotFound)?;
    let artifact = ArtifactRepository::builder()
        .connection(connection)
        .build()
        .by_number(&task.id, number)
        .await?
        .ok_or(ArtifactError::NotFound)?;
    Ok((task, artifact))
}

fn completed(artifact: artifact::Model, title: String) -> Result<ArtifactCompleted, ArtifactError> {
    Ok(ArtifactCompleted::builder()
        .artifact(Artifact::try_from(artifact)?)
        .task_title(title)
        .build())
}
