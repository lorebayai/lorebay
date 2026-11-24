use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::agent_files::helpers::{complete, register, upload_ok};
use crate::helpers::{Server, authorize_for, request};
use crate::mcp::tools::call;

use super::helpers::{SPACE, TASKS, append, create, read, scratchpad, setup};

#[tokio::test]
async fn get_task_preserves_each_writer_after_takeover_and_person_cancel() {
    let (server, person, agent) = setup().await;
    let peer = authorize_for(&server, &person, "Continuing agent").await;
    let task = create(&server, &agent, SPACE, "Shared work").await;
    let original = read(&server, &person, "TK-1").await;
    assert_actor(&original, Some("Research agent"));
    assert_eq!(original["state_changed_at"], original["created_at"]);
    let first = append(&server, &agent, &task, "First step").await;
    let registered = register(&server, &agent, &task).await;
    upload_ok(&server, &registered, "First report").await;
    let held = call(
        &server,
        &peer,
        "claim_task",
        json!({"task_id":task["task_id"]}),
    )
    .await;
    assert_eq!(held["state_changed_at"], original["state_changed_at"]);
    assert_actor(&held, Some("Research agent"));
    let second = append(&server, &peer, &task, "Second step").await;
    complete_ok(&server, &peer, &task, &registered).await;
    let other = register(&server, &peer, &task).await;
    upload_ok(&server, &other, "Second report").await;
    complete_ok(&server, &peer, &task, &other).await;
    let changed = wait_for_review(&server, &peer, &task).await;
    let checkpoints = json!([
        {"time":first["time"],"agent_name":"Research agent","content":"First step"},
        {"time":second["time"],"agent_name":"Continuing agent","content":"Second step"}
    ]);
    assert_eq!(scratchpad(&server, &person, "TK-1").await, checkpoints);
    let fetched = call(
        &server,
        &peer,
        "get_task",
        json!({"space":SPACE,"key":"TK-1","include_scratchpad":true}),
    )
    .await;
    assert_eq!(fetched["scratchpad"]["data"], checkpoints);
    assert_eq!(fetched["files"][0]["agent_name"], "Research agent");
    assert_eq!(fetched["files"][1]["agent_name"], "Continuing agent");
    let canceled = cancel_task(&server, &person, &task).await;
    assert_actor(&canceled, None);
    assert_eq!(canceled["state_changed_at"], canceled["ended_at"]);
    assert!(canceled["state_changed_at"].as_i64() >= changed["state_changed_at"].as_i64());
    let fetched = call(
        &server,
        &peer,
        "get_task",
        json!({"space":SPACE,"key":"TK-1"}),
    )
    .await;
    assert_eq!(fetched, canceled);
    assert_eq!(read(&server, &person, "TK-1").await, canceled);
    server.stop().await;
}

fn assert_actor(task: &Value, agent: Option<&str>) {
    assert_eq!(task["state_changed_by"], "alice");
    assert_eq!(task["state_changed_agent_name"], json!(agent));
    assert!(task["state_changed_at"].as_i64().unwrap() > 0);
}

async fn complete_ok(server: &Server, agent: &Value, task: &Value, registered: &Value) {
    let response = complete(server, agent, task, registered["key"].as_str().unwrap()).await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn wait_for_review(server: &Server, peer: &Value, task: &Value) -> Value {
    let changed = call(
        server,
        peer,
        "update_task",
        json!({"task_id":task["task_id"],"state":"waiting","state_reason":"Awaiting review"}),
    )
    .await;
    assert_actor(&changed, Some("Continuing agent"));
    let repeated = call(
        server,
        peer,
        "update_task",
        json!({"task_id":task["task_id"],"state":"waiting","result":"Report"}),
    )
    .await;
    assert_eq!(repeated["state_changed_at"], changed["state_changed_at"]);
    changed
}

async fn cancel_task(server: &Server, person: &Value, task: &Value) -> Value {
    let response = request(
        server,
        person,
        Method::PATCH,
        &format!("{TASKS}/{}", task["task_id"].as_str().unwrap()),
        json!({"state":"canceled","state_reason":"Review stopped"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["data"].clone()
}
