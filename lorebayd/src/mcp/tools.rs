use rmcp::model::CallToolResult;
use serde::Serialize;
use serde_json::{Value, json};

use crate::api::{
    ApiError, Envelope, Meta, NoteAddedResponse, ProblemDetails, SpaceResponse, TaskResponse,
    TaskSummaryResponse, invalid_text,
};
use crate::domain::caller::VerifiedCaller;
use crate::domain::pagination::Page;
use crate::domain::{
    NoteContent, TaskFilters, TaskTitle, add_note, create_task, list_spaces, list_tasks, read_task,
};
use crate::server::ServerState;

use super::arguments::Arguments;

pub(crate) async fn call(
    state: &ServerState,
    caller: &VerifiedCaller,
    name: &str,
    arguments: Arguments,
) -> Result<Value, ApiError> {
    match name {
        "list_spaces" => {
            let page = list_spaces(&state.database, caller, arguments.page()?).await?;
            value(page_response::<_, SpaceResponse>(page))
        }
        "create_task" => {
            let title = TaskTitle::try_from(arguments.string("title")?)
                .map_err(|error| invalid_text("title", error))?;
            let task =
                create_task(&state.database, caller, &arguments.string("space")?, title).await?;
            Ok(json!({"key":task.key, "task_id":task.task_id, "task_title":task.task_title}))
        }
        "list_tasks" => {
            let filters = TaskFilters::builder()
                .person(None)
                .agent(None)
                .status(None)
                .own_open(false)
                .keyword(None)
                .build();
            let page = list_tasks(
                &state.database,
                caller,
                &arguments.string("space")?,
                arguments.page()?,
                filters,
            )
            .await?;
            value(page_response::<_, TaskSummaryResponse>(page))
        }
        "get_task" => {
            let task = read_task(
                &state.database,
                caller,
                &arguments.string("space")?,
                &arguments.string("key")?,
                arguments.scratchpad()?,
            )
            .await?;
            value(TaskResponse::from(task))
        }
        "add_note" => {
            let content = NoteContent::try_from(arguments.string("content")?)
                .map_err(|error| invalid_text("content", error))?;
            let note = add_note(
                &state.database,
                caller,
                None,
                &arguments.string("task_id")?,
                content,
            )
            .await?;
            value(NoteAddedResponse::from(note))
        }
        _ => Err(ApiError::Internal),
    }
}

pub(crate) fn error_result(error: ApiError) -> CallToolResult {
    let problem = ProblemDetails::from(error);
    CallToolResult::structured_error(json!({
        "code":problem.code, "message":problem.title, "errors":problem.errors,
    }))
}

fn page_response<T, R: From<T>>(page: Page<T>) -> Envelope<Vec<R>> {
    Envelope::builder()
        .data(page.items.into_iter().map(R::from).collect::<Vec<_>>())
        .meta(Meta::builder().next_cursor(page.next_cursor).build())
        .build()
}

fn value(data: impl Serialize) -> Result<Value, ApiError> {
    serde_json::to_value(data).map_err(|error| {
        tracing::error!(%error, "MCP result serialization failed");
        ApiError::Internal
    })
}
