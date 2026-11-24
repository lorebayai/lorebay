use lorebay_lib_core::{
    KEYWORD_MAX_CHARACTERS, KNOWLEDGE_CONTENT_MAX_BYTES, KNOWLEDGE_NAME_MAX_CHARACTERS,
    KNOWLEDGE_SUMMARY_MAX_CHARACTERS, PersonRequestStatus,
};
use rmcp::model::Tool;
use serde_json::{Value, json};

use super::{ToolName, paged, space, tool};

pub(super) fn list_knowledge() -> (ToolName, Tool) {
    tool(
        ToolName::ListKnowledge,
        "List one level of a space's Knowledge with entry names, keys and summaries and next-level prefixes. With no prefix, list the top level; pass a returned prefix to go down one level. A keyword searches names, summaries and content across the space; prefix and keyword cannot combine. Repeat the same filters with cursor to read the next page.",
        paged(json!({
            "space":space(),
            "prefix":{"type":"string", "minLength":1, "maxLength":KNOWLEDGE_NAME_MAX_CHARACTERS + 1},
            "keyword":keyword()
        })),
        &["space"],
        true,
    )
}

pub(super) fn read_knowledge() -> (ToolName, Tool) {
    tool(
        ToolName::ReadKnowledge,
        "Read the latest version of one Knowledge entry in a space, including its content and summary. Provide exactly one of name or key, such as KN-1. Deleted entries are not found.",
        json!({"space":space(), "name":name(), "key":{"type":"string"}}),
        &["space"],
        true,
    )
}

pub(super) fn create_person_request() -> (ToolName, Tool) {
    tool(
        ToolName::CreatePersonRequest,
        "Request a Knowledge change using task_id, not a space or task key. action create requires name, summary and content. action change requires exactly one of target_name or target_key and at least one replacement name, summary or content. action delete requires exactly one target and no replacement fields. Content is nonempty Markdown, at most 1048576 UTF-8 bytes. Only your party's tasks accept submissions, including finished tasks. Set wait=true from your working task to wait for its decision. Only the holder submits from a held task. Returns the pending request and task title; Knowledge changes only after a person approves it.",
        json!({
            "task_id":{"type":"string"},
            "wait":{"type":"boolean", "default":false},
            "action":{"type":"string", "enum":["create", "change", "delete"]},
            "target_name":name(), "target_key":{"type":"string"}, "name":name(),
            "summary":{"type":"string", "minLength":1, "maxLength":KNOWLEDGE_SUMMARY_MAX_CHARACTERS},
            "content":{"type":"string", "minLength":1, "maxLength":KNOWLEDGE_CONTENT_MAX_BYTES,
                "description":"Nonempty Markdown, at most 1048576 UTF-8 bytes."}
        }),
        &["task_id", "action"],
        false,
    )
}

pub(super) fn list_person_requests() -> (ToolName, Tool) {
    tool(
        ToolName::ListPersonRequests,
        "List a space's requests, newest first, with action, status, requester, target and proposed name and summary. Filter by status and keyword; a keyword matches the proposed name, proposed summary or target name. Repeat the same filters with cursor to read the next page.",
        paged(json!({
            "space":space(), "keyword":keyword(),
            "status":{"type":"string", "enum":[PersonRequestStatus::Pending.as_str(),
                PersonRequestStatus::Approved.as_str(), PersonRequestStatus::Rejected.as_str()]}
        })),
        &["space"],
        true,
    )
}

pub(super) fn get_person_request() -> (ToolName, Tool) {
    tool(
        ToolName::GetPersonRequest,
        "Read one request in a space by key, such as RQ-1, including proposed content, status, rejection reason and, while pending, whether its target changed since it was requested.",
        json!({"space":space(), "key":{"type":"string"}}),
        &["space", "key"],
        true,
    )
}

fn name() -> Value {
    json!({"type":"string", "minLength":1, "maxLength":KNOWLEDGE_NAME_MAX_CHARACTERS,
        "description":"Entry name with nonempty slash-separated levels; no dot levels, control characters or surrounding whitespace."})
}

fn keyword() -> Value {
    json!({"type":"string", "minLength":1, "maxLength":KEYWORD_MAX_CHARACTERS})
}
