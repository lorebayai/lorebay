use std::sync::LazyLock;

use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::{Value, json};

use lorebay_lib_core::{
    ARTIFACT_FILE_NAME_MAX_BYTES, ARTIFACT_SUMMARY_MAX_CHARACTERS, KEYWORD_MAX_CHARACTERS,
    NOTE_MAX_CHARACTERS, RESULT_MAX_CHARACTERS, TITLE_MAX_CHARACTERS, TaskStatus,
};
use lorebay_lib_core::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ToolName {
    AddNote,
    CompleteArtifact,
    CreateArtifact,
    CreateTask,
    GetArtifactUrl,
    GetTask,
    ListSpaces,
    ListTasks,
    UpdateTask,
}

impl ToolName {
    fn as_str(self) -> &'static str {
        match self {
            Self::AddNote => "add_note",
            Self::CompleteArtifact => "complete_artifact",
            Self::CreateArtifact => "create_artifact",
            Self::CreateTask => "create_task",
            Self::GetArtifactUrl => "get_artifact_url",
            Self::GetTask => "get_task",
            Self::ListSpaces => "list_spaces",
            Self::ListTasks => "list_tasks",
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
            "List your spaces with keys, names, summaries, your agent role and caller_person, the name of the person you act for. Pass caller_person as person to list_tasks to find that person's tasks, including ended tasks. Follow next_cursor with cursor to read the next page.",
            paged(json!({})),
            &[],
            true,
        ),
        tool(
            ToolName::CreateTask,
            "Create a task in a space such as SPACE-1 with a nonempty title. Save the returned task_id for writes; the key is for reads. Returns the task title too.",
            json!({"space": space(), "title": {"type":"string", "minLength":1, "maxLength":TITLE_MAX_CHARACTERS}}),
            &["space", "title"],
            false,
        ),
        tool(
            ToolName::UpdateTask,
            "Change a task using task_id, not a task key. Provide at least one of title, status or result. Reopen with status in progress; result replaces the whole Markdown result and an empty string clears it. Returns the updated task and its title.",
            json!({
                "task_id":{"type":"string"},
                "title":{"type":"string", "minLength":1, "maxLength":TITLE_MAX_CHARACTERS},
                "status":{"type":"string", "enum":[TaskStatus::InProgress.as_str(), TaskStatus::Ended.as_str()]},
                "result":{"type":"string", "maxLength":RESULT_MAX_CHARACTERS}
            }),
            &["task_id"],
            false,
        ),
        tool(
            ToolName::ListTasks,
            "List a space's tasks, newest activity first. Filter by person, agent, status or keyword in titles/results. own_open selects only your party's tasks that are not ended and cannot combine with person or status. Rows include task_id for writes. Repeat the same filters with cursor to read the next page.",
            task_list_properties(),
            &["space"],
            true,
        ),
        tool(
            ToolName::GetTask,
            "Read a task by space and key such as TK-1, including its result and artifacts. Set include_scratchpad to true to read notes; page_size and cursor then page the Scratchpad.",
            paged(
                json!({"space":space(), "key":{"type":"string"}, "include_scratchpad":{"type":"boolean", "default":false}}),
            ),
            &["space", "key"],
            true,
        ),
        tool(
            ToolName::AddNote,
            "Append a nonempty Markdown note using task_id, not a task key. Only your party's tasks accept writes. Ended tasks must be reopened first. Returns the note time and task title.",
            json!({"task_id":{"type":"string"}, "content":{"type":"string", "minLength":1, "maxLength":NOTE_MAX_CHARACTERS}}),
            &["task_id", "content"],
            false,
        ),
        create_artifact(),
        complete_artifact(),
        get_artifact_url(),
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
        "status":{"type":"string", "enum":[TaskStatus::InProgress.as_str(), TaskStatus::Ended.as_str()]},
        "own_open":{"type":"boolean", "default":false},
        "keyword":{"type":"string", "minLength":1, "maxLength":KEYWORD_MAX_CHARACTERS}
    }))
}

fn create_artifact() -> (ToolName, Tool) {
    tool(
        ToolName::CreateArtifact,
        "Register an artifact using task_id, a file name and a summary. Returns its key, upload_url, method, headers and task title. Upload the file bytes with an HTTP PUT to upload_url carrying every returned header; files never pass through MCP. HTTP 412 means the file is already in storage: call complete_artifact with the returned key. Ended tasks must be reopened before registration.",
        json!({
            "task_id":{"type":"string"},
            "file_name":{"type":"string", "minLength":1, "maxLength":ARTIFACT_FILE_NAME_MAX_BYTES, "description":"Nonempty file name, at most 255 UTF-8 bytes."},
            "summary":{"type":"string", "minLength":1, "maxLength":ARTIFACT_SUMMARY_MAX_CHARACTERS}
        }),
        &["task_id", "file_name", "summary"],
        false,
    )
}

fn complete_artifact() -> (ToolName, Tool) {
    tool(
        ToolName::CompleteArtifact,
        "Report an artifact upload complete using task_id and its artifact key, such as AR-1. Call after a successful upload or HTTP 412. Checks that the file is in storage and returns the artifact with its size and the current task title. Repeating completion succeeds without changing it; a registered upload may be completed after the task ends.",
        json!({"task_id":{"type":"string"}, "key":{"type":"string"}}),
        &["task_id", "key"],
        false,
    )
}

fn get_artifact_url() -> (ToolName, Tool) {
    tool(
        ToolName::GetArtifactUrl,
        "Get a short-lived download URL and its expiry for a completed artifact in a space, using its key such as AR-1. Everyone in the space can read its artifacts. Download the file over HTTP GET using the returned url; files never pass through MCP.",
        json!({"space":space(), "key":{"type":"string"}}),
        &["space", "key"],
        true,
    )
}
