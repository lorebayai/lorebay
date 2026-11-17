use reqwest::Method;
use serde_json::{Value, json};

use crate::args::{OutputMode, SpaceCommand};
use crate::constants::DATA;
use crate::credentials::Store;
use crate::error::Result;
use crate::http::Http;
use crate::output::{Field, Fields, Outcome};

use super::{Listing, api_path, list_page};

pub(super) async fn run(command: SpaceCommand, store: &Store, mode: OutputMode) -> Result<Outcome> {
    match command {
        SpaceCommand::Change {
            space,
            name,
            summary,
        } => change(store, &space, name, summary).await,
        SpaceCommand::Create { name, summary } => create(store, name, summary).await,
        SpaceCommand::List(page) => {
            list_page(
                store,
                page,
                mode,
                Listing::builder()
                    .path(api_path(&[SPACES])?)
                    .command("space list")
                    .columns(vec!["Key", "Name", "Summary", "Role"])
                    .fields(vec!["key", "name", "summary", "role"])
                    .build(),
            )
            .await
        }
        SpaceCommand::Show { space } => show(store, &space).await,
    }
}

const SPACES: &str = "spaces";

async fn change(
    store: &Store,
    space: &str,
    name: Option<String>,
    summary: Option<String>,
) -> Result<Outcome> {
    let mut request = json!({});
    if let Some(name) = name {
        request["name"] = json!(name);
    }
    if let Some(summary) = summary {
        request["summary"] = json!(summary);
    }
    let body = Http::new()?
        .authorized(
            store,
            Method::PATCH,
            &api_path(&[SPACES, space])?,
            Some(&request),
        )
        .await?;
    Ok(space_output(body))
}

async fn create(store: &Store, name: String, summary: String) -> Result<Outcome> {
    let body = Http::new()?
        .authorized(
            store,
            Method::POST,
            &api_path(&[SPACES])?,
            Some(&json!({"name": name, "summary": summary})),
        )
        .await?;
    Ok(space_output(body))
}

async fn show(store: &Store, space: &str) -> Result<Outcome> {
    let body = Http::new()?
        .authorized(store, Method::GET, &api_path(&[SPACES, space])?, None)
        .await?;
    Ok(space_output(body))
}

fn space_output(body: Value) -> Outcome {
    let fields = Fields::builder()
        .values(
            [
                ("Key", "key"),
                ("Name", "name"),
                ("Summary", "summary"),
                ("Role", "role"),
            ]
            .into_iter()
            .map(|(label, field)| {
                let value = body[DATA][field].as_str().unwrap_or_default().to_owned();
                if field == "summary" {
                    Field::Multiline(label, value)
                } else {
                    Field::SingleLine(label, value)
                }
            })
            .collect::<Vec<_>>(),
        )
        .build();
    Outcome::new(body, fields)
}
