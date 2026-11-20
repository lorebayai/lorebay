use reqwest::Method;
use serde_json::{Value, json};
use url::form_urlencoded;

use crate::args::{ContentOptions, KnowledgeCommand, OutputMode, Page};
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
            name,
            summary,
            content: source,
        } => {
            change(
                store,
                &space,
                &entry,
                [
                    ("name", name),
                    ("summary", summary),
                    ("content", content::read(source)?),
                ],
            )
            .await
        }
        KnowledgeCommand::Create {
            space,
            name,
            summary,
            content: source,
        } => create(store, &space, name, summary, source).await,
        KnowledgeCommand::Delete { space, entry, yes } => {
            delete(
                store,
                &entry_path(&space, &entry)?,
                &format!("knowledge entry {entry} in {space}"),
                yes,
                "Knowledge entry deleted.",
            )
            .await
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
        KnowledgeCommand::Show { space, entry } => {
            request(store, Method::GET, &entry_path(&space, &entry)?, None).await
        }
    }
}

const KNOWLEDGE: &str = "knowledge";
const SPACES: &str = "spaces";

async fn change(
    store: &Store,
    space: &str,
    entry: &str,
    fields: [(&str, Option<String>); 3],
) -> Result<Outcome> {
    let body = fields
        .into_iter()
        .filter_map(|(field, value)| value.map(|value| (field.to_owned(), json!(value))))
        .collect();
    request(
        store,
        Method::PATCH,
        &entry_path(space, entry)?,
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

fn entry_path(space: &str, entry: &str) -> Result<String> {
    if entry
        .strip_prefix("KN-")
        .and_then(|number| number.parse::<i64>().ok())
        .is_some_and(|number| number > 0 && entry == format!("KN-{number}"))
    {
        return api_path(&[SPACES, space, KNOWLEDGE, "by-key", entry]);
    }
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("name", entry)
        .finish();
    Ok(format!(
        "{}?{query}",
        api_path(&[SPACES, space, KNOWLEDGE, "entry"])?
    ))
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
