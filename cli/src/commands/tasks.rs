use reqwest::Method;
use serde_json::{Value, json};

use crate::args::{OutputMode, Page, TaskCommand, TaskFilters};
use crate::constants::DATA;
use crate::credentials::Store;
use crate::error::Result;
use crate::http::Http;
use crate::output::{self, Field, Fields, Outcome, Render, Table};

use super::{Listing, api_path, delete, display_field, display_value, list_page};

pub(super) async fn run(command: TaskCommand, store: &Store, mode: OutputMode) -> Result<Outcome> {
    match command {
        TaskCommand::Cancel {
            space,
            task_id,
            reason,
        } => cancel(store, &space, &task_id, &reason).await,
        TaskCommand::Plan { space, title } => plan(store, &space, &title).await,
        TaskCommand::Delete {
            space,
            task_id,
            yes,
        } => {
            delete(
                store,
                &api_path(&[SPACES, &space, TASKS, &task_id])?,
                &format!("task {task_id} in {space}"),
                yes,
                "Task deleted.",
            )
            .await
        }
        TaskCommand::List {
            space,
            page,
            filters,
        } => list(store, &space, page, filters, mode).await,
        TaskCommand::Scratchpad { space, task, page } => {
            scratchpad(store, &space, &task, page, mode).await
        }
        TaskCommand::Results { space, task, page } => {
            results(store, &space, &task, page, mode).await
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

const OWN_OPEN: &str = "own_open";
const SCRATCHPAD: &str = "scratchpad";
const SPACES: &str = "spaces";
const TASKS: &str = "tasks";

async fn cancel(store: &Store, space: &str, task_id: &str, reason: &str) -> Result<Outcome> {
    let body = Http::new()?
        .authorized(
            store,
            Method::PATCH,
            &api_path(&[SPACES, space, TASKS, task_id])?,
            Some(&json!({"state": "canceled", "state_reason":reason})),
        )
        .await?;
    Ok(task_output(body))
}

async fn list(
    store: &Store,
    space: &str,
    page: Page,
    filters: TaskFilters,
    mode: OutputMode,
) -> Result<Outcome> {
    let query = filter_query(filters);
    let mut command = format!("task list {}", output::shell_word(space));
    for (name, value) in &query {
        if matches!(*name, OWN_OPEN | "claimable") {
            command.push_str(&format!(" --{}", name.replace('_', "-")));
        } else {
            command.push_str(&format!(" --{name} {}", output::shell_word(value)));
        }
    }
    list_page(
        store,
        page,
        mode,
        Listing::builder()
            .path(api_path(&[SPACES, space, TASKS])?)
            .command(command)
            .query(query)
            .columns(vec![
                "Key",
                "Title",
                "State",
                "Person",
                "Agent name",
                "Holder",
                "Reason",
                "Planner",
                "Planner agent",
                "Last activity at",
            ])
            .fields(vec![
                "key",
                "title",
                "state",
                "person",
                "agent_name",
                "holder",
                "state_reason",
                "planner",
                "planner_agent_name",
                "last_activity_at",
            ])
            .build(),
    )
    .await
}

async fn scratchpad(
    store: &Store,
    space: &str,
    task: &str,
    page: Page,
    mode: OutputMode,
) -> Result<Outcome> {
    list_page(
        store,
        page,
        mode,
        Listing::builder()
            .path(api_path(&[SPACES, space, TASKS, task, SCRATCHPAD])?)
            .command(format!(
                "task scratchpad {} {}",
                output::shell_word(space),
                output::shell_word(task)
            ))
            .columns(vec!["Time", "Agent name", "Content"])
            .fields(vec!["time", "agent_name", "content"])
            .multiline_columns(vec![2])
            .build(),
    )
    .await
}

async fn results(
    store: &Store,
    space: &str,
    task: &str,
    page: Page,
    mode: OutputMode,
) -> Result<Outcome> {
    list_page(
        store,
        page,
        mode,
        Listing::builder()
            .path(api_path(&[SPACES, space, TASKS, task, "results"])?)
            .command(format!(
                "task results {} {}",
                output::shell_word(space),
                output::shell_word(task)
            ))
            .columns(vec![
                "Version",
                "Person",
                "Agent name",
                "Created at",
                "Result",
            ])
            .fields(vec![
                "version",
                "person",
                "agent_name",
                "created_at",
                "result",
            ])
            .multiline_columns(vec![4])
            .build(),
    )
    .await
}

fn filter_query(filters: TaskFilters) -> Vec<(&'static str, String)> {
    let mut query = Vec::new();
    for (name, value) in [
        ("person", filters.person),
        ("agent", filters.agent),
        (
            "state",
            filters.state.map(|state| state.as_str().to_owned()),
        ),
        ("keyword", filters.keyword),
    ] {
        if let Some(value) = value {
            query.push((name, value));
        }
    }
    if filters.claimable {
        query.push(("claimable", "true".to_owned()));
    }
    if filters.own_open {
        query.push((OWN_OPEN, "true".to_owned()));
    }
    query
}

fn task_output(body: Value) -> Outcome {
    let fields = Fields::builder()
        .values(
            [
                ("Key", "key"),
                ("Task ID", "task_id"),
                ("Title", "title"),
                ("State", "state"),
                ("Person", "person"),
                ("Agent name", "agent_name"),
                ("Holder", "holder"),
                ("Reason", "state_reason"),
                ("State changed by", "state_changed_by"),
                ("State changed agent", "state_changed_agent_name"),
                ("State changed at", "state_changed_at"),
                ("Planner", "planner"),
                ("Planner agent", "planner_agent_name"),
                ("Created at", "created_at"),
                ("Started at", "started_at"),
                ("Ended at", "ended_at"),
                ("Last activity at", "last_activity_at"),
                ("Result", "result"),
                ("Result version", "result_version"),
                ("Files", "files"),
            ]
            .into_iter()
            .map(|(label, field)| {
                let value = display_field(&body[DATA], field);
                if field == "files" {
                    Field::Multiline(label, task_files(&body[DATA][field]))
                } else if matches!(field, "title" | "result") {
                    Field::Multiline(label, value.replace('\n', "\n  "))
                } else {
                    Field::SingleLine(label, value)
                }
            })
            .collect::<Vec<_>>(),
        )
        .build();
    Outcome::new(body, fields)
}

fn task_files(files: &Value) -> String {
    let rows: Vec<_> = files
        .as_array()
        .into_iter()
        .flatten()
        .map(|file| {
            vec![
                display_value(&file["key"]),
                display_value(&file["file_name"]),
                format!("{} bytes", display_value(&file["size"])),
                display_value(&file["summary"]),
                display_value(&file["agent_name"]),
                display_field(file, "created_at"),
                display_field(file, "completed_at"),
            ]
        })
        .collect();
    if rows.is_empty() {
        return String::new();
    }
    let table = Table::builder()
        .columns(vec![
            "Key",
            "Name",
            "Size",
            "Summary",
            "Agent name",
            "Created at",
            "Completed at",
        ])
        .rows(rows)
        .build()
        .human();
    format!("\n  {}", table.replace('\n', "\n  "))
}

async fn plan(store: &Store, space: &str, title: &str) -> Result<Outcome> {
    let mut body = Http::new()?
        .authorized(
            store,
            Method::POST,
            &api_path(&[SPACES, space, TASKS])?,
            Some(&json!({"title":title,"planned":true})),
        )
        .await?;
    body[DATA]["title"] = json!(title);
    let values = [("Key", "key"), ("Task ID", "task_id"), ("Title", "title")]
        .into_iter()
        .map(|(label, field)| Field::SingleLine(label, display_value(&body[DATA][field])))
        .collect::<Vec<_>>();
    Ok(Outcome::new(body, Fields::builder().values(values).build()))
}
