use std::process::Output;

use reqwest::Method;
use serde_json::{Value, json};

use super::{Server, create};

pub async fn task_context() -> (Server, String, Value) {
    let server = Server::start().await;
    server.login().await;
    let space = create(&server, "Team", "Shared work").await;
    let agent = server.agent("Task agent").await;
    (server, space, agent)
}

pub async fn create_task(server: &Server, space: &str, agent: &Value, title: &str) -> Value {
    server
        .json(
            Method::POST,
            &format!("/api/v1/spaces/{space}/tasks"),
            json!({"title": title}),
            agent["access_token"].as_str(),
        )
        .await["data"]
        .clone()
}

pub async fn append_note(
    server: &Server,
    space: &str,
    task: &Value,
    agent: &Value,
    content: &str,
) -> i64 {
    let id = task["task_id"].as_str().unwrap();
    server
        .json(
            Method::POST,
            &format!("/api/v1/spaces/{space}/tasks/{id}/notes"),
            json!({"content": content}),
            agent["access_token"].as_str(),
        )
        .await["data"]["time"]
        .as_i64()
        .unwrap()
}

pub async fn task_pages(server: &Server, command: &[&str], mode: &str) -> (Output, Output, String) {
    let mut arguments = command.to_vec();
    arguments.extend(["--page-size", "2", "--output", mode]);
    let first = server.run(&arguments).await;
    super::assert_success(&first);
    let hint_command = format!(
        "task {} {}",
        command[1],
        command[2..]
            .iter()
            .map(|segment| format!("'{segment}'"))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let cursor = next_cursor(&first, &hint_command, mode);
    arguments.extend(["--cursor", &cursor]);
    let last = server.run(&arguments).await;
    super::assert_success(&last);
    assert!(last.stderr.is_empty());
    (first, last, cursor)
}

pub fn assert_task_error(output: &Output, mode: &str, code: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
    if mode == "json" {
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], code);
        assert_eq!(
            error["status"],
            if code == "request.invalid_query" {
                400
            } else {
                404
            }
        );
        if code == "request.invalid_query" {
            assert!(!error["errors"].as_array().unwrap().is_empty());
        }
    } else {
        assert!(!output.stderr.is_empty());
    }
}

fn next_cursor(output: &Output, command: &str, mode: &str) -> String {
    let hint = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(hint.starts_with(&format!(
        "Next page: lorebay {command} --page-size 2 --cursor '"
    )));
    assert!(hint.ends_with(&format!(" --output {mode}\n")));
    hint.split(" --cursor '")
        .nth(1)
        .unwrap()
        .split('\'')
        .next()
        .unwrap()
        .to_owned()
}
