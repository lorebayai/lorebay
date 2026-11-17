use sea_orm::{ConnectionTrait, DbErr};
use thiserror::Error;

use crate::entity::task::Model;
use crate::repository::TaskRepository;
use crate::repository::participant::ParticipantRepository;

use super::caller::VerifiedCaller;
use super::{SpaceAccessError, SpaceRole, check_space};

pub(crate) const TASK_READ_ROLES: &[SpaceRole] = &[
    SpaceRole::Owner,
    SpaceRole::Admin,
    SpaceRole::Contributor,
    SpaceRole::Viewer,
];
pub(crate) const TASK_WRITE_ROLES: &[SpaceRole] =
    &[SpaceRole::Owner, SpaceRole::Admin, SpaceRole::Contributor];

#[derive(Debug, Error)]
pub(crate) enum TaskAccessError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error("Set the task back to in progress before appending a note")]
    Ended,
    #[error("Task not found")]
    NotFound,
    #[error("The space role does not allow this operation")]
    PermissionRequired,
    #[error(transparent)]
    Space(#[from] SpaceAccessError),
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TaskAction {
    AppendNote,
    Change,
    Close,
}

pub(crate) async fn check_task<C: ConnectionTrait>(
    connection: &C,
    caller: &VerifiedCaller,
    task_id: &str,
    path_space: Option<&str>,
    action: TaskAction,
) -> Result<Model, TaskAccessError> {
    let path_space = match path_space {
        Some(key) => Some(check_space(connection, caller, key, TASK_READ_ROLES).await?),
        None => None,
    };
    let task = TaskRepository::builder()
        .connection(connection)
        .build()
        .by_random_id(task_id)
        .await?
        .filter(|task| {
            path_space
                .as_ref()
                .is_none_or(|space| space.space.id == task.space_id)
        })
        .ok_or(TaskAccessError::NotFound)?;
    let participant = ParticipantRepository::builder()
        .connection(connection)
        .build()
        .by_space_and_account(&task.space_id, caller.account_id())
        .await?;
    let participant = participant.ok_or(TaskAccessError::NotFound)?;
    let role = SpaceRole::for_caller(&participant.role, caller)?;
    let may_close_any =
        matches!(action, TaskAction::Close) && matches!(role, SpaceRole::Owner | SpaceRole::Admin);
    if task.account_id != caller.account_id() && !may_close_any {
        return Err(TaskAccessError::PermissionRequired);
    }
    if role == SpaceRole::Viewer {
        return Err(TaskAccessError::PermissionRequired);
    }
    if matches!(action, TaskAction::AppendNote) && task.status == "ended" {
        return Err(TaskAccessError::Ended);
    }
    Ok(task)
}
