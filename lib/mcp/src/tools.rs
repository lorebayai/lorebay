use lorebay_lib_core::{
    Caller, CheckpointContent, LibraryState, Page, TaskRead, TaskTarget, TaskTitle, add_checkpoint,
    change_task, claim_task, complete_agent_file, create_task, get_agent_file_download_url,
    list_spaces, list_tasks, plan_task, prepare_agent_file_completion, read_task,
    register_agent_file, release_task,
};
use serde::Serialize;
use serde_json::{Value, json};

use super::arguments::Arguments;
use super::catalog::ToolName;
use super::error::ToolError;

mod knowledge;

pub(super) async fn call(
    state: &LibraryState,
    caller: &Caller,
    name: ToolName,
    arguments: Arguments,
) -> Result<Value, ToolError> {
    match name {
        ToolName::ListSpaces => page_response(list_spaces(state, caller, arguments.page()?).await?),
        ToolName::CreateTask => create(state, caller, &arguments).await,
        ToolName::ClaimTask | ToolName::ReleaseTask => hold(state, caller, name, &arguments).await,
        ToolName::UpdateTask => update(state, caller, &arguments).await,
        ToolName::ListTasks => {
            let (options, filters) = arguments.tasks()?;
            page_response(
                list_tasks(state, caller, &arguments.string("space")?, options, filters).await?,
            )
        }
        ToolName::GetTask => task_response(
            read_task(
                state,
                caller,
                &arguments.string("space")?,
                &arguments.string("key")?,
                arguments.include()?,
            )
            .await?,
        ),
        ToolName::AddCheckpoint => append(state, caller, &arguments).await,
        ToolName::CreateAgentFile => register(state, caller, &arguments).await,
        ToolName::CompleteAgentFile => complete(state, caller, &arguments).await,
        ToolName::GetAgentFileUrl => value(
            get_agent_file_download_url(
                state,
                caller,
                &arguments.string("space")?,
                &arguments.string("key")?,
            )
            .await?,
        ),
        ToolName::ListKnowledge => knowledge::list(state, caller, &arguments).await,
        ToolName::ReadKnowledge => knowledge::read(state, caller, &arguments).await,
        ToolName::CreatePersonRequest => knowledge::propose(state, caller, &arguments).await,
        ToolName::ListPersonRequests => knowledge::person_requests(state, caller, &arguments).await,
        ToolName::GetPersonRequest => knowledge::person_request(state, caller, &arguments).await,
    }
}

async fn register(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    let task_id = arguments.string("task_id")?;
    let input = arguments.agent_file()?;
    let registration = state
        .run_write(async |write| register_agent_file(write, caller, target(&task_id), input).await)
        .await?;
    value(registration.issue_upload(state).await?)
}

async fn complete(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    let task_id = arguments.string("task_id")?;
    let key = arguments.string("key")?;
    let prepared = prepare_agent_file_completion(state, caller, target(&task_id), &key).await?;
    let completion = state
        .run_write(async |write| {
            complete_agent_file(write, caller, target(&task_id), prepared).await
        })
        .await?;
    value(completion.into_result()?)
}

async fn create(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    let title = TaskTitle::try_from(arguments.string("title")?)
        .map_err(|error| ToolError::task_text("title", error))?;
    let space = arguments.string("space")?;
    let task = state
        .run_write(async |write| {
            if arguments.enabled("planned") {
                plan_task(write, caller, &space, title).await
            } else {
                create_task(write, caller, &space, title).await
            }
        })
        .await?;
    value(task)
}

async fn update(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    let task_id = arguments.string("task_id")?;
    let changes = arguments.changes()?;
    let read = state
        .run_write(async |write| change_task(write, caller, target(&task_id), changes).await)
        .await?;
    let task_title = read.task.title.clone();
    let mut response = task_response(read)?;
    response["task_title"] = json!(task_title);
    Ok(response)
}

async fn append(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    let task_id = arguments.string("task_id")?;
    let content = CheckpointContent::try_from(arguments.string("content")?)
        .map_err(|error| ToolError::task_text("content", error))?;
    let checkpoint = state
        .run_write(async |write| add_checkpoint(write, caller, target(&task_id), content).await)
        .await?;
    value(checkpoint)
}

fn target(task_id: &str) -> TaskTarget<'_> {
    TaskTarget {
        path_space: None,
        task_id,
    }
}

fn task_response(read: TaskRead) -> Result<Value, ToolError> {
    let mut result = value(read.task)?;
    result["files"] = value(read.files)?;
    if let Some(scratchpad) = read.scratchpad {
        result["scratchpad"] = page_response(scratchpad)?;
    }
    if let Some(results) = read.results {
        result["results"] = page_response(results)?;
    }
    Ok(result)
}

fn page_response<T: Serialize>(page: Page<T>) -> Result<Value, ToolError> {
    let meta = match page.next_cursor {
        Some(next_cursor) => json!({"next_cursor": next_cursor}),
        None => json!({}),
    };
    Ok(json!({"data": value(page.items)?, "meta": meta}))
}

fn value(data: impl Serialize) -> Result<Value, ToolError> {
    serde_json::to_value(data).map_err(|error| {
        tracing::error!(%error, "MCP result serialization failed");
        ToolError::internal()
    })
}

async fn hold(
    state: &LibraryState,
    caller: &Caller,
    name: ToolName,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    let task_id = arguments.string("task_id")?;
    let read = state
        .run_write(async |write| {
            if name == ToolName::ClaimTask {
                claim_task(write, caller, target(&task_id)).await
            } else {
                release_task(write, caller, target(&task_id)).await
            }
        })
        .await?;
    task_response(read)
}
