use sea_orm::DbErr;
use thiserror::Error;

use crate::entity::space_task::Model;
use crate::repository::{SpaceRepository, TaskRepository};
use crate::{Caller, LibraryState, OperationHandle};

use super::space_access::{SpaceAccessError, SpaceRole, role_in_space, tenant_space};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TaskAccessError {
    #[error(transparent)]
    Database(#[from] DbErr),
    #[error("Set the task back to in progress before {0}")]
    Ended(&'static str),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Task not found")]
    NotFound,
    #[error("The space role does not allow this operation")]
    PermissionRequired,
    #[error(transparent)]
    Space(#[from] SpaceAccessError),
}

impl TaskAccessError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Database(_) | Self::Internal(_) => "server.internal",
            Self::Ended(_) => "task.ended",
            Self::NotFound => "task.not_found",
            Self::PermissionRequired => "space.permission_required",
            Self::Space(error) => error.code(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TaskAction {
    AppendCheckpoint,
    Change,
    Close,
    CompleteAgentFile,
    Delete,
    DeleteAgentFile,
    DeleteCheckpoint,
    RegisterAgentFile,
    SubmitPersonRequest,
}

#[derive(Clone, Copy, Debug, typed_builder::TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(super) struct TaskCheck<'a> {
    pub task_id: &'a str,
    pub path_space: Option<&'a str>,
    pub action: TaskAction,
}

pub(super) async fn check_task(
    state: &LibraryState,
    handle: OperationHandle<'_>,
    caller: &Caller,
    check: TaskCheck<'_>,
) -> Result<Model, TaskAccessError> {
    let connection = handle.transaction();
    let tasks = TaskRepository::builder().connection(connection).build();
    let path = match check.path_space {
        Some(key) => Some(tenant_space(connection, caller, key).await?),
        None => None,
    };
    let task = tasks.by_random_id(check.task_id).await?;
    if let Some(task) = task.as_ref() {
        let space = SpaceRepository::builder()
            .connection(connection)
            .build()
            .by_id(&task.space_id)
            .await?;
        if space.is_none_or(|space| space.tenant_id != caller.tenant_id) {
            return Err(TaskAccessError::NotFound);
        }
    }
    let path_role = match path.as_ref() {
        Some(space) => Some(
            role_in_space(state, handle, caller, &space.id)
                .await?
                .ok_or(SpaceAccessError::NotFound)?,
        ),
        None => None,
    };
    let task = task
        .filter(|task| path.as_ref().is_none_or(|space| task.space_id == space.id))
        .ok_or(TaskAccessError::NotFound)?;
    let role = match path_role {
        Some(role) => role,
        None => role_in_space(state, handle, caller, &task.space_id)
            .await?
            .ok_or(TaskAccessError::NotFound)?,
    };
    check_party(&task, caller, role, check.action)?;
    let task = match (handle, check.action) {
        (OperationHandle::Write(_), TaskAction::DeleteAgentFile | TaskAction::DeleteCheckpoint)
        | (OperationHandle::Read(_), _) => task,
        (OperationHandle::Write(_), _) => tasks
            .lock_by_id(&task.id)
            .await?
            .ok_or(TaskAccessError::NotFound)?,
    };
    check_task_state(&task, check.action)?;
    Ok(task)
}

fn check_party(
    task: &Model,
    caller: &Caller,
    role: SpaceRole,
    action: TaskAction,
) -> Result<(), TaskAccessError> {
    let may_manage_others = matches!(
        action,
        TaskAction::Close
            | TaskAction::Delete
            | TaskAction::DeleteAgentFile
            | TaskAction::DeleteCheckpoint
    ) && matches!(role, SpaceRole::Owner | SpaceRole::Admin);
    if (task.person_id != caller.person_id() && !may_manage_others) || role == SpaceRole::Viewer {
        return Err(TaskAccessError::PermissionRequired);
    }
    Ok(())
}

fn check_task_state(task: &Model, action: TaskAction) -> Result<(), TaskAccessError> {
    match (action, task.status.as_str()) {
        (TaskAction::AppendCheckpoint, "ended") => {
            Err(TaskAccessError::Ended("appending a checkpoint"))
        }
        (TaskAction::RegisterAgentFile, "ended") => {
            Err(TaskAccessError::Ended("registering a file"))
        }
        _ => Ok(()),
    }
}
