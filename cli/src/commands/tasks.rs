use reqwest::Method;
use serde_json::Value;

use crate::args::{OutputMode, Page, TaskCommand};
use crate::constants::DATA;
use crate::credentials::Store;
use crate::error::Result;
use crate::http::Http;
use crate::output::{self, Field, Fields, Outcome};

use super::{Listing, api_path, display_value, list_page};

pub(super) async fn run(command: TaskCommand, store: &Store, mode: OutputMode) -> Result<Outcome> {
    match command {
        TaskCommand::List { space, page } => list(store, &space, page, mode).await,
        TaskCommand::Scratchpad { space, task, page } => {
            list_page(
                store,
                page,
                mode,
                Listing::builder()
                    .path(api_path(&[SPACES, &space, TASKS, &task, SCRATCHPAD])?)
                    .command(format!(
                        "task scratchpad {} {}",
                        output::shell_word(&space),
                        output::shell_word(&task)
                    ))
                    .columns(vec!["Time", "Content"])
                    .fields(vec!["time", "content"])
                    .multiline_columns(vec![1])
                    .build(),
            )
            .await
        }
        TaskCommand::Show { space, task } => {
            let body = Http::new()?
                .authorized(
                    store,
                    Method::GET,
                    &api_path(&[SPACES, &space, TASKS, &task])?,
                    None,
                )
                .await?;
            Ok(task_output(body))
        }
    }
}

const SCRATCHPAD: &str = "scratchpad";
const SPACES: &str = "spaces";
const TASKS: &str = "tasks";

async fn list(store: &Store, space: &str, page: Page, mode: OutputMode) -> Result<Outcome> {
    list_page(
        store,
        page,
        mode,
        Listing::builder()
            .path(api_path(&[SPACES, space, TASKS])?)
            .command(format!("task list {}", output::shell_word(space)))
            .columns(vec![
                "Key",
                "Title",
                "Status",
                "Person",
                "Agent name",
                "Last activity at",
            ])
            .fields(vec![
                "key",
                "title",
                "status",
                "person",
                "agent_name",
                "last_activity_at",
            ])
            .build(),
    )
    .await
}

fn task_output(body: Value) -> Outcome {
    let fields = Fields::builder()
        .values(
            [
                ("Key", "key"),
                ("Task ID", "task_id"),
                ("Title", "title"),
                ("Status", "status"),
                ("Person", "person"),
                ("Agent name", "agent_name"),
                ("Started at", "started_at"),
                ("Ended at", "ended_at"),
                ("Last activity at", "last_activity_at"),
                ("Result", "result"),
                ("Artifacts", "artifacts"),
            ]
            .into_iter()
            .map(|(label, field)| {
                let value = display_value(&body[DATA][field]);
                if matches!(field, "title" | "result") {
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
