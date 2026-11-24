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
    #[error("Task is already held by {0}")]
    Held(String),
    #[error("Task is held by {0} in another party")]
    HeldByAnotherParty(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
    #[error("Task not found")]
    NotFound,
    #[error("Only the holder {0} may write; claim the task to take it over")]
    NotHolder(String),
    #[error("The space role does not allow this operation")]
    PermissionRequired,
    #[error(transparent)]
    Space(#[from] SpaceAccessError),
    #[error("Operation is not allowed while the task is {0}; {hint}", hint = invalid_state_hint(.0))]
    StateInvalid(String),
}

impl TaskAccessError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Database(_) | Self::Internal(_) => "server.internal",
            Self::Held(_) | Self::HeldByAnotherParty(_) => "task.held",
            Self::NotHolder(_) => "task.not_holder",
            Self::StateInvalid(_) => "task.state_invalid",
            Self::NotFound => "task.not_found",
            Self::PermissionRequired => "space.permission_required",
            Self::Space(error) => error.code(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TaskAction {
    AppendCheckpoint,
    Cancel,
    Change,
    Claim,
    CompleteAgentFile,
    Delete,
    DeleteAgentFile,
    DeleteCheckpoint,
    RegisterAgentFile,
    Release,
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
    let observed_state = task.state.clone();
    let task = match (handle, check.action) {
        (OperationHandle::Write(_), TaskAction::DeleteAgentFile | TaskAction::DeleteCheckpoint)
        | (OperationHandle::Read(_), _) => task,
        (OperationHandle::Write(_), _) => tasks
            .lock_by_id(&task.id)
            .await?
            .ok_or(TaskAccessError::NotFound)?,
    };
    check_party(&task, caller, role, check.action)?;
    if check.action == TaskAction::Claim && observed_state == "planned" && task.state != "planned" {
        return Err(TaskAccessError::Held(
            task.holder.clone().unwrap_or_default(),
        ));
    }
    if check.action == TaskAction::Change && caller.agent_name.is_none() {
        return Err(TaskAccessError::PermissionRequired);
    }
    check_task_state(&task, caller, check.action)?;
    Ok(task)
}

fn check_party(
    task: &Model,
    caller: &Caller,
    role: SpaceRole,
    action: TaskAction,
) -> Result<(), TaskAccessError> {
    if role == SpaceRole::Viewer {
        return Err(TaskAccessError::PermissionRequired);
    }
    if action == TaskAction::Claim {
        if !matches!(task.state.as_str(), "planned" | "working" | "waiting") {
            return Err(TaskAccessError::StateInvalid(task.state.clone()));
        }
        if task.state == "planned" || task.person_id == caller.person_id() {
            return Ok(());
        }
        return Err(TaskAccessError::HeldByAnotherParty(
            task.holder.clone().unwrap_or_default(),
        ));
    }
    let may_manage_others = matches!(
        action,
        TaskAction::Cancel
            | TaskAction::Delete
            | TaskAction::DeleteAgentFile
            | TaskAction::DeleteCheckpoint
    ) && matches!(role, SpaceRole::Owner | SpaceRole::Admin);
    if (task.person_id != caller.person_id() && !may_manage_others) || role == SpaceRole::Viewer {
        return Err(TaskAccessError::PermissionRequired);
    }
    Ok(())
}

fn check_task_state(
    task: &Model,
    caller: &Caller,
    action: TaskAction,
) -> Result<(), TaskAccessError> {
    let held = matches!(task.state.as_str(), "working" | "waiting");
    let needs_holder = matches!(
        action,
        TaskAction::AppendCheckpoint
            | TaskAction::Change
            | TaskAction::RegisterAgentFile
            | TaskAction::Release
            | TaskAction::SubmitPersonRequest
    ) || (action == TaskAction::Cancel && caller.agent_name.is_some());
    if held && needs_holder && task.holder != caller.agent_name {
        return Err(TaskAccessError::NotHolder(
            task.holder.clone().unwrap_or_default(),
        ));
    }
    let valid = match action {
        TaskAction::AppendCheckpoint | TaskAction::RegisterAgentFile => held,
        TaskAction::Release => task.state == "working",
        TaskAction::Claim => held || task.state == "planned",
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(TaskAccessError::StateInvalid(task.state.clone()))
    }
}

fn invalid_state_hint(state: &str) -> &'static str {
    if matches!(state, "failed" | "canceled") {
        "create a new task"
    } else {
        "claim or reopen it first"
    }
}
