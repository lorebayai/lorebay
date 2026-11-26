use lorebay_lib_core::{
    Caller, KnowledgeChanges, LibraryState, PersonRequestSource, PersonRequestSpace,
    list_knowledge, list_person_requests, read_knowledge_entry, read_person_request_with_space,
    revise_person_request_with_space, submit_person_request_with_space,
};
use rmcp::model::CallToolResult;
use serde::Serialize;
use serde_json::{Value, json};

use super::{Arguments, ToolError, page_response, target, value};

pub(super) async fn list(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    page_response(
        list_knowledge(
            state,
            caller,
            &arguments.string("space")?,
            arguments.page()?,
            arguments.knowledge_filters()?,
        )
        .await?,
    )
}

pub(super) async fn read(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    value(
        read_knowledge_entry(
            state,
            caller,
            &arguments.string("space")?,
            arguments.knowledge_target()?,
        )
        .await?,
    )
}

pub(super) async fn propose(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<CallToolResult, ToolError> {
    let task_id = arguments.string("task_id")?;
    let content = arguments.person_request()?;
    let source = if arguments.enabled("wait") {
        PersonRequestSource::WaitingTask(target(&task_id))
    } else {
        PersonRequestSource::Task(target(&task_id))
    };
    let (request, space) = state
        .run_write(async |write| {
            submit_person_request_with_space(write, caller, source, content).await
        })
        .await?;
    request_result(request, space)
}

pub(super) async fn person_requests(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    page_response(
        list_person_requests(
            state,
            caller,
            &arguments.string("space")?,
            arguments.page()?,
            arguments.person_request_filters()?,
        )
        .await?,
    )
}

pub(super) async fn person_request(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<CallToolResult, ToolError> {
    let (request, space) = read_person_request_with_space(
        state,
        caller,
        &arguments.string("space")?,
        &arguments.string("key")?,
    )
    .await?;
    request_result(request, space)
}

pub(super) async fn revise(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<CallToolResult, ToolError> {
    let space = arguments.string("space")?;
    let key = arguments.string("key")?;
    let name = arguments.optional_string("name");
    let summary = arguments.optional_string("summary");
    let content = arguments.optional_string("content");
    let changes = if name.is_none() && summary.is_none() && content.is_none() {
        None
    } else {
        Some(KnowledgeChanges::new(name, summary, content)?)
    };
    let (request, space) = state
        .run_write(async |write| {
            revise_person_request_with_space(write, caller, &space, &key, changes).await
        })
        .await?;
    request_result(request, space)
}

fn request_result(
    request: impl Serialize,
    space: PersonRequestSpace,
) -> Result<CallToolResult, ToolError> {
    let mut result = CallToolResult::structured(value(request)?);
    if let Some(content) = &mut result.structured_content {
        content["space_key"] = json!(space.key);
        content["space_name"] = json!(space.name);
    }
    Ok(result)
}
