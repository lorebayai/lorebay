use reqwest::Method;
use serde_json::{Value, json};
use url::form_urlencoded;

use crate::args::{ContentOptions, KnowledgeChanges, KnowledgeCommand, OutputMode, Page};
use crate::constants::DATA;
use crate::credentials::Store;
use crate::error::Result;
use crate::http::Http;
use crate::output::{self, Field, Fields, Outcome};

use super::{Listing, api_path, content, delete, display_value, list_page};

pub(super) async fn run(
    command: KnowledgeCommand,
    store: &Store,
    mode: OutputMode,
) -> Result<Outcome> {
    match command {
        KnowledgeCommand::Change {
            space,
            entry,
            changes,
            version,
        } => change(store, &space, &entry, changes, version).await,
        KnowledgeCommand::Create {
            space,
            name,
            summary,
            content: source,
        } => create(store, &space, name, summary, source).await,
        KnowledgeCommand::Delete {
            space,
            entry,
            yes,
            version,
        } => delete_entry(store, &space, &entry, version, yes).await,
        KnowledgeCommand::Deleted { space, page } => deleted(store, &space, page, mode).await,
        KnowledgeCommand::History { space, entry, page } => {
            history(store, &space, &entry, page, mode).await
        }
        KnowledgeCommand::List {
            space,
            prefix,
            keyword,
            page,
        } => {
            list(
                store,
                &space,
                page,
                [("prefix", prefix), ("keyword", keyword)],
                mode,
            )
            .await
        }
        KnowledgeCommand::Rollback {
            space,
            entry,
            version,
            source_version,
        } => rollback(store, &space, &entry, source_version, version).await,
        KnowledgeCommand::Show {
            space,
            entry,
            version,
        } => show(store, &space, &entry, version).await,
    }
}

pub(super) fn is_entry_key(entry: &str) -> bool {
    entry
        .strip_prefix("KN-")
        .and_then(|number| number.parse::<i64>().ok())
        .is_some_and(|number| number > 0 && entry == format!("KN-{number}"))
}

const KNOWLEDGE: &str = "knowledge";
const SPACES: &str = "spaces";

async fn change(
    store: &Store,
    space: &str,
    entry: &str,
    changes: KnowledgeChanges,
    version: i64,
) -> Result<Outcome> {
    let fields = [
        ("name", changes.name),
        ("summary", changes.summary),
        ("content", content::read(changes.content)?),
    ];
    let mut body: serde_json::Map<String, Value> = fields
        .into_iter()
        .filter_map(|(field, value)| value.map(|value| (field.to_owned(), json!(value))))
        .collect();
    body.insert("version".to_owned(), json!(version));
    request(
        store,
        Method::PATCH,
        &entry_path(space, entry, &[])?,
        Some(&Value::Object(body)),
    )
    .await
}

async fn create(
    store: &Store,
    space: &str,
    name: String,
    summary: String,
    source: ContentOptions,
) -> Result<Outcome> {
    let body = json!({"name": name, "summary": summary, "content": content::read(source)?});
    request(
        store,
        Method::POST,
        &api_path(&[SPACES, space, KNOWLEDGE])?,
        Some(&body),
    )
    .await
}

async fn deleted(store: &Store, space: &str, page: Page, mode: OutputMode) -> Result<Outcome> {
    list_page(
        store,
        page,
        mode,
        Listing::builder()
            .path(api_path(&[SPACES, space, KNOWLEDGE, "deleted"])?)
            .command(format!("knowledge deleted {}", output::shell_word(space)))
            .columns(vec!["Name", "Key", "Deleted by", "Deleted at"])
            .fields(vec!["name", "key", "deleted_by", "deleted_at"])
            .build(),
    )
    .await
}

async fn history(
    store: &Store,
    space: &str,
    entry: &str,
    page: Page,
    mode: OutputMode,
) -> Result<Outcome> {
    list_page(
        store,
        page,
        mode,
        Listing::builder()
            .path(entry_path(space, entry, &["versions"])?)
            .command(format!(
                "knowledge history {} {}",
                output::shell_word(space),
                output::shell_word(entry)
            ))
            .columns(vec![
                "Version",
                "Kind",
                "Applied by",
                "Applied at",
                "Request",
            ])
            .fields(vec![
                "version",
                "kind",
                "applied_by",
                "applied_at",
                "request_key",
            ])
            .build(),
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
    let mut command = format!("knowledge list {}", output::shell_word(space));
    for (name, value) in &query {
        command.push_str(&format!(" --{name} {}", output::shell_word(value)));
    }
    list_page(
        store,
        page,
        mode,
        Listing::builder()
            .path(api_path(&[SPACES, space, KNOWLEDGE])?)
            .command(command)
            .query(query)
            .columns(vec!["Name", "Key", "Summary"])
            .fields(vec!["name", "key", "summary"])
            .build(),
    )
    .await
}

fn entry_path(space: &str, entry: &str, suffix: &[&str]) -> Result<String> {
    let key = is_entry_key(entry);
    let mut segments = vec![SPACES, space, KNOWLEDGE];
    if key {
        segments.extend(["by-key", entry]);
    } else {
        segments.push("entry");
    }
    segments.extend_from_slice(suffix);
    let path = api_path(&segments)?;
    if key {
        return Ok(path);
    }
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("name", entry)
        .finish();
    Ok(format!("{path}?{query}"))
}

fn versioned_path(space: &str, entry: &str, version: i64) -> Result<String> {
    let path = entry_path(space, entry, &[])?;
    let separator = if path.contains('?') { '&' } else { '?' };
    Ok(format!("{path}{separator}version={version}"))
}

async fn rollback(
    store: &Store,
    space: &str,
    entry: &str,
    source_version: i64,
    version: i64,
) -> Result<Outcome> {
    request(
        store,
        Method::POST,
        &entry_path(space, entry, &["rollback"])?,
        Some(&json!({"version": source_version, "target_version": version})),
    )
    .await
}

async fn show(store: &Store, space: &str, entry: &str, version: Option<i64>) -> Result<Outcome> {
    let path = match version {
        Some(version) => entry_path(space, entry, &["versions", &version.to_string()])?,
        None => entry_path(space, entry, &[])?,
    };
    request(store, Method::GET, &path, None).await
}

async fn request(
    store: &Store,
    method: Method,
    path: &str,
    body: Option<&Value>,
) -> Result<Outcome> {
    let body = Http::new()?.authorized(store, method, path, body).await?;
    let values = [
        ("Name", "name"),
        ("Key", "key"),
        ("Version", "version"),
        ("Kind", "kind"),
        ("Summary", "summary"),
        ("Content", "content"),
        ("Applied by", "applied_by"),
        ("Applied at", "applied_at"),
    ]
    .into_iter()
    .map(|(label, field)| {
        let value = display_value(&body[DATA][field]);
        if field == "content" {
            Field::Multiline(label, value)
        } else {
            Field::SingleLine(label, value)
        }
    })
    .collect::<Vec<_>>();
    Ok(Outcome::new(body, Fields::builder().values(values).build()))
}

async fn delete_entry(
    store: &Store,
    space: &str,
    entry: &str,
    version: i64,
    yes: bool,
) -> Result<Outcome> {
    delete(
        store,
        &versioned_path(space, entry, version)?,
        &format!("knowledge entry {entry} in {space}"),
        yes,
        "Knowledge entry deleted.",
    )
    .await
}
