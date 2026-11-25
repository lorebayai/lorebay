use lorebay_lib_core::{
    Caller, KnowledgeChanges, LibraryState, PersonRequestSource, list_knowledge,
    list_person_requests, read_knowledge_entry, read_person_request, revise_person_request,
    submit_person_request,
};
use serde_json::Value;

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
) -> Result<Value, ToolError> {
    let task_id = arguments.string("task_id")?;
    let content = arguments.person_request()?;
    value(
        state
            .run_write(async |write| {
                submit_person_request(
                    write,
                    caller,
                    if arguments.enabled("wait") {
                        PersonRequestSource::WaitingTask(target(&task_id))
                    } else {
                        PersonRequestSource::Task(target(&task_id))
                    },
                    content,
                )
                .await
            })
            .await?,
    )
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
) -> Result<Value, ToolError> {
    value(
        read_person_request(
            state,
            caller,
            &arguments.string("space")?,
            &arguments.string("key")?,
        )
        .await?,
    )
}

pub(super) async fn revise(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
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
    value(
        state
            .run_write(async |write| {
                revise_person_request(write, caller, &space, &key, changes).await
            })
            .await?,
    )
}
