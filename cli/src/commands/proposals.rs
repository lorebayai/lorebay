use reqwest::Method;
use serde_json::{Value, json};

use crate::args::{KnowledgeChanges, OutputMode, Page, ProposalCommand};
use crate::constants::DATA;
use crate::credentials::Store;
use crate::error::Result;
use crate::http::Http;
use crate::output::{self, Field, Fields, Outcome};

use super::knowledge::is_entry_key;
use super::{Listing, api_path, content, display_value, list_page};

pub(super) async fn run(
    command: ProposalCommand,
    store: &Store,
    mode: OutputMode,
) -> Result<Outcome> {
    match command {
        ProposalCommand::Approve { space, proposal } => {
            decide(store, &space, &proposal, "approve", None).await
        }
        ProposalCommand::Change {
            space,
            entry,
            changes,
        } => change(store, &space, &entry, changes).await,
        ProposalCommand::Create {
            space,
            name,
            summary,
            content: source,
        } => {
            let body = json!({"kind": "create", "name": name, "summary": summary, "content": content::read(source)?});
            submit(store, &space, &body).await
        }
        ProposalCommand::Delete { space, entry } => {
            submit(store, &space, &target("delete", &entry)).await
        }
        ProposalCommand::List {
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
        ProposalCommand::Reject {
            space,
            proposal,
            reason,
        } => {
            decide(
                store,
                &space,
                &proposal,
                "reject",
                Some(&json!({"reason": reason})),
            )
            .await
        }
        ProposalCommand::Show { space, proposal } => show(store, &space, &proposal).await,
    }
}

const PROPOSALS: &str = "proposals";
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

async fn decide(
    store: &Store,
    space: &str,
    proposal: &str,
    action: &str,
    body: Option<&Value>,
) -> Result<Outcome> {
    request(
        store,
        Method::POST,
        &api_path(&[SPACES, space, PROPOSALS, proposal, action])?,
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
    let mut command = format!("proposal list {}", output::shell_word(space));
    for (name, value) in &query {
        command.push_str(&format!(" --{name} {}", output::shell_word(value)));
    }
    list_page(
        store,
        page,
        mode,
        Listing::builder()
            .path(api_path(&[SPACES, space, PROPOSALS])?)
            .command(command)
            .query(query)
            .columns(vec!["Key", "Kind", "Status", "Target", "Proposed summary"])
            .fields(vec!["key", "kind", "status", "target", "new_summary"])
            .build(),
    )
    .await
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
        ("Kind", "kind"),
        ("Status", "status"),
        ("Target", "target"),
        ("Proposed name", "new_name"),
        ("Proposed summary", "new_summary"),
        ("Content", "new_content"),
        ("Proposed by", "proposed_by"),
        ("Agent name", "proposer_agent_name"),
        ("Source task", "from_task_key"),
        ("Created at", "created_at"),
        ("Base version", "base_version"),
        ("Changed since proposed", "changed_since_proposed"),
        ("Decided by", "decided_by"),
        ("Decided at", "decided_at"),
        ("Rejection reason", "reject_reason"),
    ]
    .into_iter()
    .map(|(label, field)| {
        let value = display_value(&body[DATA][field]);
        if field == "new_content" {
            Field::Multiline(label, value)
        } else {
            Field::SingleLine(label, value)
        }
    })
    .collect::<Vec<_>>();
    Ok(Outcome::new(body, Fields::builder().values(values).build()))
}

async fn show(store: &Store, space: &str, proposal: &str) -> Result<Outcome> {
    request(
        store,
        Method::GET,
        &api_path(&[SPACES, space, PROPOSALS, proposal])?,
        None,
    )
    .await
}

async fn submit(store: &Store, space: &str, body: &Value) -> Result<Outcome> {
    request(
        store,
        Method::POST,
        &api_path(&[SPACES, space, PROPOSALS])?,
        Some(body),
    )
    .await
}

fn target(kind: &str, entry: &str) -> Value {
    let field = if is_entry_key(entry) {
        "target_key"
    } else {
        "target_name"
    };
    json!({"kind": kind, (field): entry})
}
