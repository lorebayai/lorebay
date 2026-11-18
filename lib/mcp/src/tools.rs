use lorebay_lib_core::{
    Caller, LibraryState, NoteContent, Page, TaskRead, TaskTarget, TaskTitle, WriteOperation,
    add_note, change_task, complete_artifact, create_task, get_artifact_download_url, list_spaces,
    list_tasks, read_task, register_artifact,
};
use serde::Serialize;
use serde_json::{Value, json};

use super::arguments::Arguments;
use super::error::ToolError;

pub(super) async fn call(
    state: &LibraryState,
    caller: &Caller,
    name: &str,
    arguments: Arguments,
) -> Result<Value, ToolError> {
    match name {
        "list_spaces" => page_response(list_spaces(state, caller, arguments.page()?).await?),
        "create_task" => create(state, caller, &arguments).await,
        "update_task" => update(state, caller, &arguments).await,
        "list_tasks" => {
            let (options, filters) = arguments.tasks()?;
            page_response(
                list_tasks(state, caller, &arguments.string("space")?, options, filters).await?,
            )
        }
        "get_task" => task_response(
            read_task(
                state,
                caller,
                &arguments.string("space")?,
                &arguments.string("key")?,
                arguments.scratchpad()?,
            )
            .await?,
        ),
        "add_note" => append(state, caller, &arguments).await,
        "create_artifact" => value(
            register_artifact(
                state,
                caller,
                target(&arguments.string("task_id")?),
                arguments.artifact()?,
            )
            .await?,
        ),
        "complete_artifact" => value(
            complete_artifact(
                state,
                caller,
                target(&arguments.string("task_id")?),
                &arguments.string("key")?,
            )
            .await?,
        ),
        "get_artifact_url" => value(
            get_artifact_download_url(
                state,
                caller,
                &arguments.string("space")?,
                &arguments.string("key")?,
            )
            .await?,
        ),
        _ => Err(ToolError::internal()),
    }
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
        .run_write(WriteOperation::CreateTask, Some(caller), async |write| {
            create_task(write, caller, &space, title).await
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
        .run_write(WriteOperation::ChangeTask, Some(caller), async |write| {
            change_task(write, caller, target(&task_id), changes).await
        })
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
    let content = NoteContent::try_from(arguments.string("content")?)
        .map_err(|error| ToolError::task_text("content", error))?;
    let note = state
        .run_write(WriteOperation::AddNote, Some(caller), async |write| {
            add_note(write, caller, target(&task_id), content).await
        })
        .await?;
    value(note)
}

fn target(task_id: &str) -> TaskTarget<'_> {
    TaskTarget {
        path_space: None,
        task_id,
    }
}

fn task_response(read: TaskRead) -> Result<Value, ToolError> {
    let mut result = value(read.task)?;
    result["artifacts"] = value(read.artifacts)?;
    if let Some(scratchpad) = read.scratchpad {
        result["scratchpad"] = page_response(scratchpad)?;
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
