use sea_orm::DbErr;
use serde::Serialize;
use thiserror::Error;

use crate::Caller;
use crate::entity::space_task::Model;

use super::task_access::TaskAccessError;
use super::tasks::TaskError;
use super::time::now_millis;

pub const TASK_REASON_MAX_CHARACTERS: usize = 2000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskState {
    Canceled,
    Done,
    Failed,
    Planned,
    Waiting,
    Working,
}

#[derive(Debug, Error)]
#[error("Expected planned, working, waiting, done, failed or canceled.")]
pub struct TaskStateError;

impl TaskStateError {
    pub const FIELD: &str = "state";
    pub const FIELD_CODE: &str = "invalid";
}

impl TaskState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Canceled => "canceled",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Planned => "planned",
            Self::Waiting => "waiting",
            Self::Working => "working",
        }
    }
}

impl TryFrom<&str> for TaskState {
    type Error = TaskStateError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "canceled" => Ok(Self::Canceled),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            "planned" => Ok(Self::Planned),
            "waiting" => Ok(Self::Waiting),
            "working" => Ok(Self::Working),
            _ => Err(TaskStateError),
        }
    }
}

pub(super) fn change_state(
    task: &mut Model,
    next: TaskState,
    reason: Option<String>,
    caller: &Caller,
) -> Result<(), TaskError> {
    let current = TaskState::try_from(task.state.as_str())
        .map_err(|_| DbErr::Type("Invalid task state".into()))?;
    if current == next && reason.is_none() {
        return Ok(());
    }
    let allowed = matches!(
        (current, next),
        (TaskState::Planned, TaskState::Canceled)
            | (
                TaskState::Working,
                TaskState::Waiting | TaskState::Done | TaskState::Failed | TaskState::Canceled
            )
            | (
                TaskState::Waiting,
                TaskState::Working | TaskState::Done | TaskState::Failed | TaskState::Canceled
            )
            | (TaskState::Done, TaskState::Working)
    );
    if !allowed {
        return Err(TaskAccessError::StateInvalid(task.state.clone()).into());
    }
    validate_reason(next, reason.as_deref())?;
    task.state = next.as_str().to_owned();
    task.state_reason = reason;
    task.waiting_request_id = None;
    task.ended_at = matches!(
        next,
        TaskState::Done | TaskState::Failed | TaskState::Canceled
    )
    .then_some(now_millis()?);
    task.holder = if matches!(next, TaskState::Working | TaskState::Waiting) {
        Some(caller.agent()?.to_owned())
    } else {
        None
    };
    if current == TaskState::Done {
        task.agent_name = task.holder.clone();
    }
    Ok(())
}

fn validate_reason(state: TaskState, reason: Option<&str>) -> Result<(), TaskError> {
    let required = matches!(
        state,
        TaskState::Waiting | TaskState::Failed | TaskState::Canceled
    );
    let valid = match reason {
        Some(value) => {
            required
                && !value.trim().is_empty()
                && value.chars().count() <= TASK_REASON_MAX_CHARACTERS
                && !value.contains(['\r', '\n'])
        }
        None => !required,
    };
    if valid {
        Ok(())
    } else {
        Err(TaskError::InvalidReason)
    }
}
