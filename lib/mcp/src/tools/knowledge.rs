use lorebay_lib_core::{
    Caller, LibraryState, ProposalSource, list_knowledge, list_proposals, read_knowledge_entry,
    read_proposal, submit_proposal,
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
    let content = arguments.proposal()?;
    value(
        state
            .run_write(async |write| {
                submit_proposal(
                    write,
                    caller,
                    ProposalSource::Task(target(&task_id)),
                    content,
                )
                .await
            })
            .await?,
    )
}

pub(super) async fn proposals(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    page_response(
        list_proposals(
            state,
            caller,
            &arguments.string("space")?,
            arguments.page()?,
            arguments.proposal_filters()?,
        )
        .await?,
    )
}

pub(super) async fn proposal(
    state: &LibraryState,
    caller: &Caller,
    arguments: &Arguments,
) -> Result<Value, ToolError> {
    value(
        read_proposal(
            state,
            caller,
            &arguments.string("space")?,
            &arguments.string("key")?,
        )
        .await?,
    )
}
