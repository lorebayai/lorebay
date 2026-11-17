use reqwest::Method;
use serde_json::Value;
use url::{Position, Url};

use crate::args::{AgentCommand, Command, OutputMode, Page};
use crate::credentials::Store;
use crate::error::{Failure, Result};
use crate::http::{self, Http};
use crate::login;
use crate::output::{self, Outcome, Table};

pub(crate) async fn run(command: Command, mode: OutputMode) -> Result<Outcome> {
    let store = Store::from_environment()?;
    match command {
        Command::Login {
            server_url,
            no_browser,
        } => {
            login::login(&store, &server_url, no_browser).await?;
            Ok(Outcome::completion("Signed in."))
        }
        Command::Logout => {
            http::logout(&store).await?;
            Ok(Outcome::completion("Signed out."))
        }
        Command::Agent {
            command: AgentCommand::List(page),
        } => list(&store, page, mode).await,
        Command::Agent {
            command: AgentCommand::Revoke { name, yes },
        } => {
            if !yes {
                output::confirm(&name)?;
            }
            Http::new()?
                .authorized(&store, Method::DELETE, &agent_path(&name)?)
                .await?;
            Ok(Outcome::completion("Agent revoked."))
        }
    }
}

async fn list(store: &Store, page: Page, mode: OutputMode) -> Result<Outcome> {
    let mut url = Url::parse("http://localhost/api/v1/agents")
        .map_err(|_| Failure::local("cli.usage", "Could not construct the agents path."))?;
    if let Some(size) = page.page_size {
        url.query_pairs_mut()
            .append_pair("page_size", &size.to_string());
    }
    if let Some(cursor) = &page.cursor {
        url.query_pairs_mut().append_pair("cursor", cursor);
    }
    let path = &url[Position::BeforePath..];
    let body = Http::new()?.authorized(store, Method::GET, path).await?;
    let items = body["data"].as_array().ok_or_else(|| {
        Failure::local(
            "cli.connection",
            "The server returned an invalid agent list.",
        )
    })?;
    let rows: Vec<_> = items
        .iter()
        .map(|item| {
            vec![
                text(&item["name"]),
                text(&item["client_id"]),
                item["created_at"].to_string(),
            ]
        })
        .collect();
    let next = body["meta"]["next_cursor"]
        .as_str()
        .filter(|cursor| !cursor.is_empty())
        .map(|cursor| next_page(&page, cursor, mode));
    let mut outcome = Outcome::new(
        body,
        Table::builder()
            .columns(vec!["Name", "Client ID", "Created at"])
            .rows(rows)
            .build(),
    );
    outcome.next = next;
    Ok(outcome)
}

fn next_page(page: &Page, cursor: &str, mode: OutputMode) -> String {
    let size = page
        .page_size
        .map(|size| format!(" --page-size {size}"))
        .unwrap_or_default();
    let format = match mode {
        OutputMode::Human => "human",
        OutputMode::Json => "json",
    };
    format!(
        "Next page: lorebay agent list{size} --cursor {} --output {format}",
        output::shell_word(cursor)
    )
}

fn agent_path(name: &str) -> Result<String> {
    let mut url = Url::parse("http://localhost/api/v1/agents/")
        .map_err(|_| Failure::local("cli.usage", "Could not construct the agent path."))?;
    url.path_segments_mut()
        .map_err(|_| Failure::local("cli.usage", "Invalid agent path."))?
        .pop_if_empty()
        .push(name);
    Ok(url.path().to_owned())
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}
