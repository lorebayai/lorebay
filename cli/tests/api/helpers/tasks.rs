use std::process::Output;

use reqwest::{Method, StatusCode};
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

pub async fn read_task(server: &Server, space: &str, key: &str) -> Value {
    server
        .json(
            Method::GET,
            &format!("/api/v1/spaces/{space}/tasks/{key}"),
            Value::Null,
            server.credentials()["access_token"].as_str(),
        )
        .await["data"]
        .clone()
}

pub async fn task_with_artifacts(
    server: &Server,
    space: &str,
    agent: &Value,
    title: &str,
    result: &str,
) -> Value {
    let task = create_task(server, space, agent, title).await;
    let id = task["task_id"].as_str().unwrap();
    server
        .json(
            Method::PATCH,
            &format!("/api/v1/spaces/{space}/tasks/{id}"),
            json!({"result": result}),
            agent["access_token"].as_str(),
        )
        .await;
    let first = upload_task_artifact(server, space, &task, agent, "report.txt").await;
    let second = upload_task_artifact(server, space, &task, agent, "chart.csv").await;
    register_task_artifact(server, space, &task, agent, "pending.txt").await;
    let read = read_task(server, space, task["key"].as_str().unwrap()).await;
    assert_eq!(read["artifacts"], json!([first, second]));
    assert_eq!(read["result"], result);
    read
}

pub async fn task_id_from_show(server: &Server, space: &str, key: &str, mode: &str) -> String {
    let output = server
        .run(&["task", "show", space, key, "--output", mode])
        .await;
    super::assert_success(&output);
    assert!(output.stderr.is_empty());
    if mode == "json" {
        return super::body(&output)["data"]["task_id"]
            .as_str()
            .unwrap()
            .to_owned();
    }
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("Task ID: "))
        .unwrap()
        .to_owned()
}

const ARTIFACT_BYTES: &str = "Scenario artifact\n";

async fn register_task_artifact(
    server: &Server,
    space: &str,
    task: &Value,
    agent: &Value,
    file_name: &str,
) -> Value {
    let id = task["task_id"].as_str().unwrap();
    server
        .json(
            Method::POST,
            &format!("/api/v1/spaces/{space}/tasks/{id}/artifacts"),
            json!({"file_name": file_name, "summary": "Completed artifact"}),
            agent["access_token"].as_str(),
        )
        .await["data"]
        .clone()
}

async fn upload_task_artifact(
    server: &Server,
    space: &str,
    task: &Value,
    agent: &Value,
    file_name: &str,
) -> Value {
    let registered = register_task_artifact(server, space, task, agent, file_name).await;
    let method = Method::from_bytes(registered["method"].as_str().unwrap().as_bytes()).unwrap();
    let mut request = server
        .client
        .request(method, registered["upload_url"].as_str().unwrap())
        .body(ARTIFACT_BYTES);
    for (name, value) in registered["headers"].as_object().unwrap() {
        request = request.header(name, value.as_str().unwrap());
    }
    let response = request
        .send()
        .await
        .map_err(|_| "artifact upload failed")
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(response.bytes().await.unwrap().is_empty());
    let id = task["task_id"].as_str().unwrap();
    let key = registered["key"].as_str().unwrap();
    let mut completed = server
        .json(
            Method::POST,
            &format!("/api/v1/spaces/{space}/tasks/{id}/artifacts/{key}/complete"),
            Value::Null,
            agent["access_token"].as_str(),
        )
        .await["data"]
        .clone();
    assert_eq!(completed["key"], key);
    assert_eq!(completed["file_name"], file_name);
    assert_eq!(completed["size"], ARTIFACT_BYTES.len());
    completed.as_object_mut().unwrap().remove("task_title");
    completed
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
