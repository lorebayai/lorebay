use std::process::{Output, Stdio};

use reqwest::Method;
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::time::timeout;

use super::{
    SECOND_USERNAME, Server, WAIT, assert_success, assert_task_error, assert_usage, body,
    create_task, task_context,
};

pub const FILTER_AGENT: &str = "Filter agent 'home' /?#+%";
pub const FILTER_KEYWORD: &str = "needle 'quoted' %_\\ &?";

pub async fn filtered_tasks() -> (Server, String) {
    let (server, space, other) = task_context().await;
    let agent = server.agent(FILTER_AGENT).await;
    assert_success(
        &server
            .run(&[
                "participant",
                "add",
                &space,
                SECOND_USERNAME,
                "--role",
                "contributor",
            ])
            .await,
    );
    let teammate = server.agent_as(FILTER_AGENT, SECOND_USERNAME).await;
    create_filter_fixtures(&server, &space, &agent, &other, &teammate).await;
    (server, space)
}

pub async fn assert_filtered_pages(server: &Server, space: &str, filters: &[&str], mode: &str) {
    let mut arguments = vec!["task", "list", space];
    arguments.extend(filters);
    arguments.extend(["--page-size", "2", "--output", mode]);
    let first = server.run(&arguments).await;
    assert_success(&first);
    assert_eq!(page_keys(&first, mode), ["TK-3", "TK-2"]);
    let hint = String::from_utf8(first.stderr.clone()).unwrap();
    assert!(hint.starts_with(&format!("Next page: lorebay task list '{space}'")));
    assert!(hint.contains(" --page-size 2 --cursor '"));
    assert!(hint.ends_with(&format!(" --output {mode}\n")));
    if mode == "json" {
        let cursor = body(&first)["meta"]["next_cursor"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(hint.contains(&format!(" --cursor '{cursor}'")));
    }
    let last = run_list_hint(server, &hint).await;
    assert_success(&last);
    assert!(last.stderr.is_empty());
    assert_eq!(page_keys(&last, mode), ["TK-1"]);
    if mode == "json" {
        assert_eq!(body(&last)["meta"], json!({}));
    }
}

pub async fn assert_filter_errors(server: &mut Server, space: &str) {
    for extra in ["--person", "--status"] {
        let value = if extra == "--person" {
            server.username.as_str()
        } else {
            "ended"
        };
        let refused = server
            .run(&[
                "task",
                "list",
                space,
                "--own-open",
                extra,
                value,
                "--output",
                "json",
            ])
            .await;
        assert_task_error(&refused, "json", "request.invalid_query");
    }
    server.stop();
    for status in ["unknown", "in-progress"] {
        let refused = server
            .run(&[
                "task", "list", space, "--status", status, "--output", "json",
            ])
            .await;
        assert_usage(&refused);
    }
}

pub async fn run_list_hint(server: &Server, hint: &str) -> Output {
    let arguments = hint.trim_end().strip_prefix("Next page: lorebay ").unwrap();
    timeout(
        WAIT,
        Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("exec \"$TASK_CLI\" {arguments}"))
            .env_clear()
            .env("TASK_CLI", env!("CARGO_BIN_EXE_lorebay"))
            .env("HOME", server.directory.path().join("home"))
            .env("LOREBAY_HOME", server.home())
            .current_dir(server.directory.path())
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap()
}

async fn create_filter_fixtures(
    server: &Server,
    space: &str,
    agent: &Value,
    other: &Value,
    teammate: &Value,
) {
    for (index, (tokens, title)) in [
        (agent, "First task"),
        (agent, FILTER_KEYWORD),
        (agent, FILTER_KEYWORD),
        (other, FILTER_KEYWORD),
        (agent, "Different text"),
        (agent, FILTER_KEYWORD),
        (teammate, FILTER_KEYWORD),
    ]
    .into_iter()
    .enumerate()
    {
        let task = create_task(server, space, tokens, title).await;
        if index == 0 || index == 5 {
            let changes = if index == 0 {
                json!({"result": FILTER_KEYWORD})
            } else {
                json!({"status": "ended"})
            };
            let id = task["task_id"].as_str().unwrap();
            server
                .json(
                    Method::PATCH,
                    &format!("/api/v1/spaces/{space}/tasks/{id}"),
                    changes,
                    tokens["access_token"].as_str(),
                )
                .await;
        }
    }
}

fn page_keys(output: &Output, mode: &str) -> Vec<String> {
    if mode == "json" {
        return body(output)["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|task| task["key"].as_str().unwrap().to_owned())
            .collect();
    }
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .skip(1)
        .map(|line| line.split('\t').next().unwrap().to_owned())
        .collect()
}
