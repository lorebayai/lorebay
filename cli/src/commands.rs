use reqwest::Method;
use serde_json::Value;
use url::{Position, Url};

use crate::constants::{
    CLI_CONNECTION, CLI_USAGE, CLIENT_ID_FIELD, DATA, HUMAN_OUTPUT, JSON_OUTPUT, META,
};

use crate::args::{AgentCommand, Command, OutputMode, Page};
use crate::credentials::Store;
use crate::device;
use crate::error::{Failure, Result};
use crate::http::{self, Http};
use crate::login;
use crate::output::{self, Outcome, Table};

mod agent_files;
mod checkpoints;
mod content;
mod deletion;
mod knowledge;
mod pages;
mod proposals;
mod space_members;
mod spaces;
mod tasks;
mod users;

pub(super) use deletion::delete;
pub(super) use pages::{Listing, api_path, display_value, list_page};

pub(crate) async fn run(command: Command, mode: OutputMode) -> Result<Outcome> {
    let store = Store::from_environment()?;
    match command {
        Command::AgentFile { command } => agent_files::run(command, &store).await,
        Command::Knowledge { command } => knowledge::run(command, &store, mode).await,
        Command::Login {
            server_url,
            no_browser,
            device,
        } => {
            if device {
                device::login(&store, &server_url).await?;
            } else {
                login::login(&store, &server_url, no_browser).await?;
            }
            Ok(Outcome::completion("Signed in."))
        }
        Command::Member { command } => space_members::run(command, &store, mode).await,
        Command::Proposal { command } => proposals::run(command, &store, mode).await,
        Command::Checkpoint { command } => checkpoints::run(command, &store).await,
        Command::Space { command } => spaces::run(command, &store, mode).await,
        Command::Task { command } => tasks::run(command, &store, mode).await,
        Command::User { command } => users::run(command, &store, mode).await,
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
            let path = api_path(&[AGENTS, &name])?;
            if !yes {
                output::confirm("Revoke agent", &name)?;
            }
            Http::new()?
                .authorized(&store, Method::DELETE, &path, None)
                .await?;
            Ok(Outcome::completion("Agent revoked."))
        }
    }
}

const AGENTS: &str = "agents";

async fn list(store: &Store, page: Page, mode: OutputMode) -> Result<Outcome> {
    let mut url = Url::parse("http://localhost/api/v1/agents")
        .map_err(|_| Failure::local(CLI_USAGE, "Could not construct the agents path."))?;
    if let Some(size) = page.page_size {
        url.query_pairs_mut()
            .append_pair("page_size", &size.to_string());
    }
    if let Some(cursor) = &page.cursor {
        url.query_pairs_mut().append_pair("cursor", cursor);
    }
    let path = &url[Position::BeforePath..];
    let body = Http::new()?
        .authorized(store, Method::GET, path, None)
        .await?;
    let items = body[DATA].as_array().ok_or_else(|| {
        Failure::local(CLI_CONNECTION, "The server returned an invalid agent list.")
    })?;
    let rows: Vec<_> = items
        .iter()
        .map(|item| {
            vec![
                text(&item["name"]),
                text(&item[CLIENT_ID_FIELD]),
                item["created_at"].to_string(),
            ]
        })
        .collect();
    let next = body[META]["next_cursor"]
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
        OutputMode::Human => HUMAN_OUTPUT,
        OutputMode::Json => JSON_OUTPUT,
    };
    format!(
        "Next page: lorebay agent list{size} --cursor {} --output {format}",
        output::shell_word(cursor)
    )
}

fn text(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}
