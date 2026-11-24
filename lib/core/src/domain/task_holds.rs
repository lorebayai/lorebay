use sea_orm::{ActiveModelTrait, IntoActiveModel};

use crate::entity::space_task;
use crate::repository::TaskRepository;
use crate::{Caller, Write, WriteOperation};

use super::task_access::{TaskAccessError, TaskAction, check_task};
use super::task_state::record_state_change;
use super::tasks::{TaskError, TaskRead, TaskTarget, full_task};
use super::time::now_millis;

pub async fn claim_task(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
) -> Result<TaskRead, TaskError> {
    write.begin_operation(Some(caller))?;
    let agent = caller.agent()?.to_owned();
    let task = check_task(
        write.state(),
        write.handle(),
        caller,
        target.check(TaskAction::Claim),
    )
    .await?;
    let mut task = task;
    match task.state.as_str() {
        "planned" => {
            let time = now_millis()?;
            record_state_change(&mut task, caller, time);
            task.state = "working".to_owned();
            task.person_id = caller.person_id.clone();
            task.started_at = task.started_at.or(Some(time));
        }
        "working" | "waiting" => {}
        _ => return Err(TaskAccessError::StateInvalid(task.state).into()),
    }
    task.holder = Some(agent.clone());
    task.agent_name = Some(agent);
    save_hold(write, task, WriteOperation::ClaimTask).await
}

pub async fn release_task(
    write: &mut Write<'_>,
    caller: &Caller,
    target: TaskTarget<'_>,
) -> Result<TaskRead, TaskError> {
    write.begin_operation(Some(caller))?;
    caller.agent()?;
    let mut task = check_task(
        write.state(),
        write.handle(),
        caller,
        target.check(TaskAction::Release),
    )
    .await?;
    if task.work_recorded != 0 {
        return Err(TaskError::HasWork);
    }
    if task.planner_person_id.is_none() {
        task.planner_person_id = Some(task.person_id.clone());
        task.planner_agent_name = caller.agent_name.clone();
    }
    task.state = "planned".to_owned();
    record_state_change(&mut task, caller, now_millis()?);
    task.holder = None;
    task.agent_name = None;
    task.state_reason = None;
    task.waiting_request_id = None;
    if let Some(planner) = &task.planner_person_id {
        task.person_id = planner.clone();
    }
    save_hold(write, task, WriteOperation::ReleaseTask).await
}

async fn save_hold(
    write: &mut Write<'_>,
    task: space_task::Model,
    operation: WriteOperation,
) -> Result<TaskRead, TaskError> {
    let space = task.space_id.clone();
    let task = TaskRepository::builder()
        .connection(write.transaction())
        .build()
        .update(task.into_active_model().reset_all())
        .await?;
    let result = full_task(write.state(), write.handle(), task, None).await?;
    write.record(operation, Some(space));
    Ok(result)
}
