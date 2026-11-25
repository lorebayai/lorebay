use reqwest::Method;
use serde_json::{Value, json};

use crate::args::{ContentOptions, KnowledgeChanges, OutputMode, Page, PersonRequestCommand};
use crate::constants::DATA;
use crate::credentials::Store;
use crate::error::Result;
use crate::http::Http;
use crate::output::{self, Field, Fields, Outcome};

use super::knowledge::is_entry_key;
use super::{Listing, api_path, content, display_value, list_page};

pub(super) async fn run(
    command: PersonRequestCommand,
    store: &Store,
    mode: OutputMode,
) -> Result<Outcome> {
    match command {
        PersonRequestCommand::Approve {
            space,
            person_request,
            revision,
        } => approve(store, &space, &person_request, revision).await,
        PersonRequestCommand::Change {
            space,
            entry,
            changes,
        } => change(store, &space, &entry, changes).await,
        PersonRequestCommand::Create {
            space,
            name,
            summary,
            content: source,
        } => create(store, &space, &name, &summary, source).await,
        PersonRequestCommand::Delete { space, entry } => {
            submit(store, &space, &target("delete", &entry)).await
        }
        PersonRequestCommand::List {
            space,
            status,
            keyword,
            page,
        } => {
            list(
                store,
                &space,
                page,
                [
                    ("status", status.map(|status| status.as_str().to_owned())),
                    ("keyword", keyword),
                ],
                mode,
            )
            .await
        }
        PersonRequestCommand::Reject {
            space,
            person_request,
            reason,
            revision,
        } => reject(store, &space, &person_request, reason, revision).await,
        PersonRequestCommand::Revise {
            space,
            person_request,
            changes,
        } => revise(store, &space, &person_request, changes).await,
        PersonRequestCommand::Show {
            space,
            person_request,
        } => show(store, &space, &person_request).await,
    }
}

const PERSON_REQUESTS: &str = "requests";
const SPACES: &str = "spaces";

async fn change(
    store: &Store,
    space: &str,
    entry: &str,
    changes: KnowledgeChanges,
) -> Result<Outcome> {
    let mut body = target("change", entry);
    for (field, value) in [
        ("name", changes.name),
        ("summary", changes.summary),
        ("content", content::read(changes.content)?),
    ] {
        if let Some(value) = value {
            body[field] = json!(value);
        }
    }
    submit(store, space, &body).await
}

async fn revise(
    store: &Store,
    space: &str,
    key: &str,
    changes: KnowledgeChanges,
) -> Result<Outcome> {
    let mut body = json!({});
    for (field, value) in [
        ("name", changes.name),
        ("summary", changes.summary),
        ("content", content::read(changes.content)?),
    ] {
        if let Some(value) = value {
            body[field] = json!(value);
        }
    }
    request(
        store,
        Method::PATCH,
        &api_path(&[SPACES, space, PERSON_REQUESTS, key])?,
        Some(&body),
    )
    .await
}

async fn create(
    store: &Store,
    space: &str,
    name: &str,
    summary: &str,
    source: ContentOptions,
) -> Result<Outcome> {
    let body = json!({
        "action": "create", "name": name, "summary": summary,
        "content": content::read(source)?,
    });
    submit(store, space, &body).await
}

async fn decide(
    store: &Store,
    space: &str,
    person_request: &str,
    action: &str,
    body: Option<&Value>,
) -> Result<Outcome> {
    request(
        store,
        Method::POST,
        &api_path(&[SPACES, space, PERSON_REQUESTS, person_request, action])?,
        body,
    )
    .await
}

async fn list(
    store: &Store,
    space: &str,
    page: Page,
    filters: [(&'static str, Option<String>); 2],
    mode: OutputMode,
) -> Result<Outcome> {
    let query: Vec<_> = filters
        .into_iter()
        .filter_map(|(field, value)| value.map(|value| (field, value)))
        .collect();
    let mut command = format!("request list {}", output::shell_word(space));
    for (name, value) in &query {
        command.push_str(&format!(" --{name} {}", output::shell_word(value)));
    }
    list_page(
        store,
        page,
        mode,
        Listing::builder()
            .path(api_path(&[SPACES, space, PERSON_REQUESTS])?)
            .command(command)
            .query(query)
            .columns(vec![
                "Key",
                "Action",
                "Status",
                "Target",
                "Proposed summary",
            ])
            .fields(vec!["key", "action", "status", "target", "new_summary"])
            .display_field(person_request_field as fn(&Value, &str) -> String)
            .build(),
    )
    .await
}

fn person_request_field(person_request: &Value, field: &str) -> String {
    if field != "target" {
        return display_value(&person_request[field]);
    }
    let target = &person_request["target"];
    if target.is_object() {
        format!(
            "{} {}",
            display_value(&target["key"]),
            display_value(&target["name"])
        )
    } else {
        display_value(&person_request["new_name"])
    }
}

async fn request(
    store: &Store,
    method: Method,
    path: &str,
    body: Option<&Value>,
) -> Result<Outcome> {
    let body = Http::new()?.authorized(store, method, path, body).await?;
    let values = [
        ("Key", "key"),
        ("Action", "action"),
        ("Status", "status"),
        ("Target", "target"),
        ("Proposed name", "new_name"),
        ("Proposed summary", "new_summary"),
        ("Content", "new_content"),
        ("Requested by", "requested_by"),
        ("Submitting agent", "requester_agent_name"),
        ("Source task", "from_task_key"),
        ("Created at", "created_at"),
        ("Base version", "base_version"),
        ("Base name", "base_name"),
        ("Revision", "revision"),
        ("Outdated", "outdated"),
        ("Last revising agent", "reviser_agent_name"),
        ("Revised by", "revised_by"),
        ("Revised at", "revised_at"),
        ("Decided by", "decided_by"),
        ("Decided at", "decided_at"),
        ("Rejection reason", "reject_reason"),
    ]
    .into_iter()
    .map(|(label, field)| {
        let value = person_request_field(&body[DATA], field);
        if field == "new_content" {
            Field::Multiline(label, value)
        } else {
            Field::SingleLine(label, value)
        }
    })
    .collect::<Vec<_>>();
    Ok(Outcome::new(body, Fields::builder().values(values).build()))
}

async fn show(store: &Store, space: &str, person_request: &str) -> Result<Outcome> {
    request(
        store,
        Method::GET,
        &api_path(&[SPACES, space, PERSON_REQUESTS, person_request])?,
        None,
    )
    .await
}

async fn submit(store: &Store, space: &str, body: &Value) -> Result<Outcome> {
    request(
        store,
        Method::POST,
        &api_path(&[SPACES, space, PERSON_REQUESTS])?,
        Some(body),
    )
    .await
}

fn target(action: &str, entry: &str) -> Value {
    let field = if is_entry_key(entry) {
        "target_key"
    } else {
        "target_name"
    };
    json!({"action": action, (field): entry})
}

async fn approve(store: &Store, space: &str, key: &str, revision: i64) -> Result<Outcome> {
    decide(
        store,
        space,
        key,
        "approve",
        Some(&json!({"revision":revision})),
    )
    .await
}

async fn reject(
    store: &Store,
    space: &str,
    key: &str,
    reason: String,
    revision: i64,
) -> Result<Outcome> {
    decide(
        store,
        space,
        key,
        "reject",
        Some(&json!({"revision":revision,"reason":reason})),
    )
    .await
}
