use std::sync::LazyLock;

use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::{Value, json};

use lorebay_lib_core::{
    AGENT_FILE_NAME_MAX_BYTES, AGENT_FILE_SUMMARY_MAX_CHARACTERS, CHECKPOINT_MAX_CHARACTERS,
    KEYWORD_MAX_CHARACTERS, RESULT_MAX_CHARACTERS, TASK_REASON_MAX_CHARACTERS,
    TITLE_MAX_CHARACTERS,
};
use lorebay_lib_core::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};

mod knowledge;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ToolName {
    AddCheckpoint,
    ClaimTask,
    CompleteAgentFile,
    CreateAgentFile,
    CreatePersonRequest,
    CreateTask,
    GetAgentFileUrl,
    GetPersonRequest,
    GetTask,
    ListKnowledge,
    ListPersonRequests,
    ListSpaces,
    ListTasks,
    ReadKnowledge,
    ReleaseTask,
    UpdateTask,
}

impl ToolName {
    fn as_str(self) -> &'static str {
        match self {
            Self::AddCheckpoint => "add_checkpoint",
            Self::ClaimTask => "claim_task",
            Self::CompleteAgentFile => "complete_file",
            Self::CreateAgentFile => "create_file",
            Self::CreatePersonRequest => "create_request",
            Self::CreateTask => "create_task",
            Self::GetAgentFileUrl => "get_file_url",
            Self::GetPersonRequest => "get_request",
            Self::GetTask => "get_task",
            Self::ListKnowledge => "list_knowledge",
            Self::ListPersonRequests => "list_requests",
            Self::ListSpaces => "list_spaces",
            Self::ListTasks => "list_tasks",
            Self::ReadKnowledge => "read_knowledge",
            Self::ReleaseTask => "release_task",
            Self::UpdateTask => "update_task",
        }
    }
}

pub(crate) fn catalog() -> &'static [(ToolName, Tool)] {
    CATALOG.as_slice()
}

static CATALOG: LazyLock<Vec<(ToolName, Tool)>> = LazyLock::new(build_catalog);

fn build_catalog() -> Vec<(ToolName, Tool)> {
    vec![
        tool(
            ToolName::ListSpaces,
            "List your spaces with keys, names, summaries, your agent role and caller_person, the name of the person you act for. Pass caller_person as person to list_tasks to find that person's tasks, including finished tasks. Follow next_cursor with cursor to read the next page.",
            paged(json!({})),
            &[],
            true,
        ),
        create_task(),
        update_task(),
        list_tasks(),
        tool(
            ToolName::GetTask,
            "Read a task by space and key such as TK-1, including its result and files. Set include_scratchpad to true to read checkpoints; page_size and cursor then page the Scratchpad.",
            paged(
                json!({"space":space(), "key":{"type":"string"}, "include_scratchpad":{"type":"boolean", "default":false}}),
            ),
            &["space", "key"],
            true,
        ),
        tool(
            ToolName::AddCheckpoint,
            "Append a nonempty Markdown checkpoint using task_id, not a task key. Only the holder of a working or waiting task may append; claim or reopen first. Returns the checkpoint time and task title.",
            json!({"task_id":{"type":"string"}, "content":{"type":"string", "minLength":1, "maxLength":CHECKPOINT_MAX_CHARACTERS}}),
            &["task_id", "content"],
            false,
        ),
        tool(
            ToolName::ClaimTask,
            "Claim a planned task, moving it to your party, or take over a held task of your party. Returns its state, holder and planner. A task held by another party is refused with task.held naming its holder.",
            json!({"task_id":{"type":"string"}}),
            &["task_id"],
            false,
        ),
        tool(
            ToolName::ReleaseTask,
            "Release your working task to planned before recording any checkpoint, file or result. Recorded work is refused with task.has_work.",
            json!({"task_id":{"type":"string"}}),
            &["task_id"],
            false,
        ),
        create_agent_file(),
        complete_agent_file(),
        get_agent_file_url(),
        knowledge::list_knowledge(),
        knowledge::read_knowledge(),
        knowledge::create_person_request(),
        knowledge::list_person_requests(),
        knowledge::get_person_request(),
    ]
}

fn tool(
    name: ToolName,
    description: &'static str,
    properties: Value,
    required: &[&str],
    read_only: bool,
) -> (ToolName, Tool) {
    let schema = JsonObject::from_iter([
        ("type".into(), json!("object")),
        ("properties".into(), json!(properties)),
        ("required".into(), json!(required)),
        ("additionalProperties".into(), json!(false)),
    ]);
    let definition = Tool::new(name.as_str(), description, schema).with_annotations(
        ToolAnnotations::new()
            .read_only(read_only)
            .destructive(name == ToolName::UpdateTask)
            .open_world(false),
    );
    (name, definition)
}

fn space() -> Value {
    json!({"type":"string", "description":"Space key, such as SPACE-1."})
}

fn paged(properties: Value) -> Value {
    let mut properties = properties.as_object().cloned().unwrap_or_default();
    properties.insert("page_size".into(), json!({"type":"integer", "minimum":1, "maximum":MAX_PAGE_SIZE, "default":DEFAULT_PAGE_SIZE}));
    properties.insert(
        "cursor".into(),
        json!({"type":"string", "description":"Opaque next_cursor from the preceding page."}),
    );
    json!(properties)
}

fn task_list_properties() -> Value {
    paged(json!({
        "space":space(), "person":{"type":"string"}, "agent":{"type":"string"},
        "state":{"type":"string", "enum":["planned", "working", "waiting", "done", "failed", "canceled"]},
        "own_open":{"type":"boolean", "default":false},
        "claimable":{"type":"boolean", "default":false},
        "keyword":{"type":"string", "minLength":1, "maxLength":KEYWORD_MAX_CHARACTERS}
    }))
}

fn create_agent_file() -> (ToolName, Tool) {
    tool(
        ToolName::CreateAgentFile,
        "Register a file using task_id, a file name and a summary. Returns its key, upload_url, method, headers and task title. Upload the file's content with an HTTP PUT to upload_url carrying every returned header; file content never passes through MCP. HTTP 412 means the file's content is already stored: call complete_file with the returned key. Only the holder of a working or waiting task may register; claim or reopen first.",
        json!({
            "task_id":{"type":"string"},
            "file_name":{"type":"string", "minLength":1, "maxLength":AGENT_FILE_NAME_MAX_BYTES, "description":"Nonempty file name, at most 255 UTF-8 bytes."},
            "summary":{"type":"string", "minLength":1, "maxLength":AGENT_FILE_SUMMARY_MAX_CHARACTERS}
        }),
        &["task_id", "file_name", "summary"],
        false,
    )
}

fn complete_agent_file() -> (ToolName, Tool) {
    tool(
        ToolName::CompleteAgentFile,
        "Report a file upload complete using task_id and its file key, such as FL-1. Call after a successful upload or HTTP 412. Checks that the file content is in storage and returns the file with its size and the current task title. Repeating completion succeeds without changing it; a registered upload may be completed after the task ends.",
        json!({"task_id":{"type":"string"}, "key":{"type":"string"}}),
        &["task_id", "key"],
        false,
    )
}

fn get_agent_file_url() -> (ToolName, Tool) {
    tool(
        ToolName::GetAgentFileUrl,
        "Get a short-lived download URL and its expiry for a completed file in a space, using its key such as FL-1. Everyone in the space can read its files. Download the file's content over HTTP GET using the returned url; file content never passes through MCP.",
        json!({"space":space(), "key":{"type":"string"}}),
        &["space", "key"],
        true,
    )
}

fn create_task() -> (ToolName, Tool) {
    tool(
        ToolName::CreateTask,
        "Create a working task and hold it, or pass planned=true to queue it for a later claim. Use a space such as SPACE-1 and a nonempty title. Save the returned task_id for writes; the key is for reads. Returns the task title too.",
        json!({"space": space(), "title": {"type":"string", "minLength":1, "maxLength":TITLE_MAX_CHARACTERS}, "planned":{"type":"boolean", "default":false}}),
        &["space", "title"],
        false,
    )
}

fn update_task() -> (ToolName, Tool) {
    tool(
        ToolName::UpdateTask,
        "Change a task using task_id, not a task key. Provide at least one of title, state or result. Only the holder writes. Reopen done with state working; waiting, failed and canceled require a one-line reason; result replaces the whole Markdown result and an empty string clears it. Returns the updated task and its title.",
        json!({
            "task_id":{"type":"string"},
            "title":{"type":"string", "minLength":1, "maxLength":TITLE_MAX_CHARACTERS},
            "state":{"type":"string", "enum":["working", "waiting", "done", "failed", "canceled"]},
            "state_reason":{"type":"string", "minLength":1, "maxLength":TASK_REASON_MAX_CHARACTERS},
            "result":{"type":"string", "maxLength":RESULT_MAX_CHARACTERS}
        }),
        &["task_id"],
        false,
    )
}

fn list_tasks() -> (ToolName, Tool) {
    tool(
        ToolName::ListTasks,
        "List a space's tasks, newest activity first. Filter by person, agent, state or keyword in titles/results. own_open selects your party's working and waiting tasks and cannot combine with person or state. claimable selects planned tasks and requires a non-viewer agent. Rows include task_id for writes. Repeat the same filters with cursor to read the next page.",
        task_list_properties(),
        &["space"],
        true,
    )
}
