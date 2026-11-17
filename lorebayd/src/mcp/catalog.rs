use std::sync::LazyLock;

use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::{Value, json};

use crate::api::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};
use crate::domain::{NOTE_MAX_CHARACTERS, TITLE_MAX_CHARACTERS};

pub(crate) fn catalog() -> &'static [Tool] {
    CATALOG.as_slice()
}

static CATALOG: LazyLock<Vec<Tool>> = LazyLock::new(build_catalog);

fn build_catalog() -> Vec<Tool> {
    vec![
        tool(
            "list_spaces",
            "List your spaces with keys, names, summaries and your agent role. Follow next_cursor with cursor to read the next page.",
            paged(json!({})),
            &[],
            true,
        ),
        tool(
            "create_task",
            "Create a task in a space such as SPACE-1 with a nonempty title. Save the returned task_id for writes; the key is for reads. Returns the task title too.",
            json!({"space": space(), "title": {"type":"string", "minLength":1, "maxLength":TITLE_MAX_CHARACTERS}}),
            &["space", "title"],
            false,
        ),
        tool(
            "list_tasks",
            "List a space's tasks, newest activity first, including task_id, key, title, status, person and agent name. Follow next_cursor with cursor to read the next page.",
            paged(json!({"space": space()})),
            &["space"],
            true,
        ),
        tool(
            "get_task",
            "Read a task by space and key such as TK-1, including its result and artifacts. Set include_scratchpad to true to read notes; page_size and cursor then page the Scratchpad.",
            paged(
                json!({"space":space(), "key":{"type":"string"}, "include_scratchpad":{"type":"boolean", "default":false}}),
            ),
            &["space", "key"],
            true,
        ),
        tool(
            "add_note",
            "Append a nonempty Markdown note using task_id, not a task key. Only your party's tasks accept writes. Ended tasks must be reopened first. Returns the note time and task title.",
            json!({"task_id":{"type":"string"}, "content":{"type":"string", "minLength":1, "maxLength":NOTE_MAX_CHARACTERS}}),
            &["task_id", "content"],
            false,
        ),
    ]
}

fn tool(
    name: &'static str,
    description: &'static str,
    properties: Value,
    required: &[&str],
    read_only: bool,
) -> Tool {
    let schema = JsonObject::from_iter([
        ("type".into(), json!("object")),
        ("properties".into(), json!(properties)),
        ("required".into(), json!(required)),
        ("additionalProperties".into(), json!(false)),
    ]);
    Tool::new(name, description, schema).with_annotations(
        ToolAnnotations::new()
            .read_only(read_only)
            .destructive(false)
            .open_world(false),
    )
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
