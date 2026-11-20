use lorebay_lib_core::{
    KEYWORD_MAX_CHARACTERS, KNOWLEDGE_CONTENT_MAX_BYTES, KNOWLEDGE_NAME_MAX_CHARACTERS,
    KNOWLEDGE_SUMMARY_MAX_CHARACTERS, ProposalStatus,
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

pub(super) fn create_proposal() -> (ToolName, Tool) {
    tool(
        ToolName::CreateProposal,
        "Propose a Knowledge entry using task_id, not a space or task key. kind create requires name, summary and content. kind change requires exactly one of target_name or target_key and at least one replacement name, summary or content. kind delete requires exactly one target and no replacement fields. Content is nonempty Markdown, at most 1048576 UTF-8 bytes. Only your party's tasks accept submissions, including ended tasks. Returns the pending proposal and task title; Knowledge changes only after a person approves it.",
        json!({
            "task_id":{"type":"string"},
            "kind":{"type":"string", "enum":["create", "change", "delete"]},
            "target_name":name(), "target_key":{"type":"string"}, "name":name(),
            "summary":{"type":"string", "minLength":1, "maxLength":KNOWLEDGE_SUMMARY_MAX_CHARACTERS},
            "content":{"type":"string", "minLength":1, "maxLength":KNOWLEDGE_CONTENT_MAX_BYTES,
                "description":"Nonempty Markdown, at most 1048576 UTF-8 bytes."}
        }),
        &["task_id", "kind"],
        false,
    )
}

pub(super) fn list_proposals() -> (ToolName, Tool) {
    tool(
        ToolName::ListProposals,
        "List a space's proposals, newest first, with kind, status, proposer, target and proposed name and summary. Filter by status and keyword; a keyword matches the proposed name, proposed summary or target name. Repeat the same filters with cursor to read the next page.",
        paged(json!({
            "space":space(), "keyword":keyword(),
            "status":{"type":"string", "enum":[ProposalStatus::Pending.as_str(),
                ProposalStatus::Approved.as_str(), ProposalStatus::Rejected.as_str()]}
        })),
        &["space"],
        true,
    )
}

pub(super) fn get_proposal() -> (ToolName, Tool) {
    tool(
        ToolName::GetProposal,
        "Read one proposal in a space by key, such as PP-1, including proposed content, status, rejection reason and, while pending, whether its target changed since it was proposed.",
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
