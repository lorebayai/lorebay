use reqwest::Method;
use serde_json::{Value, json};

use crate::args::{MemberCommand, OutputMode};
use crate::constants::DATA;
use crate::credentials::Store;
use crate::error::Result;
use crate::http::Http;
use crate::output::{self, Outcome, Table};

use super::{Listing, api_path, list_page};

pub(super) async fn run(
    command: MemberCommand,
    store: &Store,
    mode: OutputMode,
) -> Result<Outcome> {
    match command {
        MemberCommand::Add {
            space,
            username,
            role,
        } => add(store, &space, &username, role.as_str()).await,
        MemberCommand::List { space, page } => {
            list_page(
                store,
                page,
                mode,
                Listing::builder()
                    .path(api_path(&[SPACES, &space, SPACE_MEMBERS])?)
                    .command(format!("member list {}", output::shell_word(&space)))
                    .columns(COLUMNS.to_vec())
                    .fields(FIELDS.to_vec())
                    .build(),
            )
            .await
        }
        MemberCommand::Remove { space, username } => remove(store, &space, &username).await,
        MemberCommand::SetRole {
            space,
            username,
            role,
        } => set_role(store, &space, &username, role.as_str()).await,
    }
}

const COLUMNS: [&str; 4] = ["Type", "Username", "Agent name", "Role"];
const FIELDS: [&str; 4] = ["type", "username", "agent_name", "role"];
const SPACE_MEMBERS: &str = "members";
const SPACES: &str = "spaces";

async fn add(store: &Store, space: &str, username: &str, role: &str) -> Result<Outcome> {
    let body = Http::new()?
        .authorized(
            store,
            Method::POST,
            &api_path(&[SPACES, space, SPACE_MEMBERS, username])?,
            Some(&json!({"role": role})),
        )
        .await?;
    Ok(space_member_output(body))
}

async fn remove(store: &Store, space: &str, username: &str) -> Result<Outcome> {
    let body = Http::new()?
        .authorized(
            store,
            Method::DELETE,
            &api_path(&[SPACES, space, SPACE_MEMBERS, username])?,
            None,
        )
        .await?;
    let mut outcome = Outcome::completion("Member removed.");
    outcome.body = body;
    Ok(outcome)
}

async fn set_role(store: &Store, space: &str, username: &str, role: &str) -> Result<Outcome> {
    let body = Http::new()?
        .authorized(
            store,
            Method::PATCH,
            &api_path(&[SPACES, space, SPACE_MEMBERS, username])?,
            Some(&json!({"role": role})),
        )
        .await?;
    Ok(space_member_output(body))
}

fn space_member_output(body: Value) -> Outcome {
    let row = FIELDS
        .iter()
        .map(|field| body[DATA][field].as_str().unwrap_or_default().to_owned())
        .collect();
    Outcome::new(
        body,
        Table::builder()
            .columns(COLUMNS.to_vec())
            .rows(vec![row])
            .build(),
    )
}
