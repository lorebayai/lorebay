use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, authorize_for, login, request};
use crate::tasks::helpers::{self as tasks, SPACE, TASKS};

use super::{NEWEST_VERSION, call, call_error, call_version, initialize};

#[tokio::test]
async fn call_update_task_returns_task_title_and_sets_status() {
    let (server, person, agent) = tasks::setup().await;
    let peer = authorize_for(&server, &person, "Continuing agent").await;
    for version in [NEWEST_VERSION, "2025-03-26", "2025-06-18", "2025-11-25"] {
        if version != NEWEST_VERSION {
            initialize(&server, &peer, version).await;
        }
        let task = call_version(
            &server,
            &agent,
            "create_task",
            json!({"space":SPACE, "title":version}),
            version,
        )
        .await;
        call(
            &server,
            &peer,
            "claim_task",
            json!({"task_id":task["task_id"]}),
        )
        .await;
        update_and_reopen(&server, &person, &peer, &task, version).await;
    }
    server.stop().await;
}

#[tokio::test]
async fn call_update_task_fails_when_arguments_are_invalid() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Original").await;
    let original = tasks::read(&server, &person, "TK-1").await;
    for changes in [
        json!({}),
        json!({"unknown":"private-sentinel"}),
        json!({"space":SPACE, "title":"Forbidden"}),
        json!({"title":""}),
        json!({"title":"   "}),
        json!({"title":"x".repeat(201)}),
        json!({"title":null}),
        json!({"title":false}),
        json!({"state":"closed"}),
        json!({"state":null}),
        json!({"state":false}),
        json!({"result":"x".repeat(100_001)}),
        json!({"result":null}),
        json!({"result":false}),
    ] {
        let mut arguments = changes.clone();
        arguments["task_id"] = task["task_id"].clone();
        let error = call_error(&server, &agent, "update_task", arguments).await;
        let response = request(
            &server,
            &agent,
            Method::PATCH,
            &format!("{TASKS}/{}", task["task_id"].as_str().unwrap()),
            changes,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let problem = response.json::<Value>().await.unwrap();
        assert_eq!(error["code"], problem["code"]);
        assert_eq!(error["message"], problem["title"]);
        assert_eq!(error["errors"], problem["errors"]);
        assert!(!error.to_string().contains("private-sentinel"));
    }
    assert_eq!(tasks::read(&server, &person, "TK-1").await, original);
    server.stop().await;
}

#[tokio::test]
async fn call_update_task_fails_when_task_is_outside_writable_party() {
    let (server, person, owner_agent) = tasks::setup().await;
    let alice_task = tasks::create(&server, &owner_agent, SPACE, "Alice task").await;
    let alice_original = tasks::read(&server, &person, "TK-1").await;
    let bob = login(&server, "bob").await;
    let bob_agent = authorize_for(&server, &bob, "Bob agent").await;
    let unknown = call_error(
        &server,
        &bob_agent,
        "update_task",
        json!({"task_id":"unknown", "state":"done"}),
    )
    .await;
    assert_eq!(unknown["code"], "task.not_found");
    let alice_close = json!({"task_id":alice_task["task_id"], "state":"done"});
    assert_eq!(
        call_error(&server, &bob_agent, "update_task", alice_close.clone()).await,
        unknown
    );
    assert_eq!(
        call_error(
            &server,
            &owner_agent,
            "update_task",
            json!({"task_id":"TK-1", "state":"done"})
        )
        .await,
        unknown
    );
    tasks::space_member(&server, &person, Method::POST, "contributor").await;
    assert_eq!(
        call_error(&server, &bob_agent, "update_task", alice_close.clone()).await["code"],
        "space.permission_required"
    );
    let bob_task = tasks::create(&server, &bob_agent, SPACE, "Bob task").await;
    let bob_original = tasks::read(&server, &person, "TK-2").await;
    let bob_close = json!({"task_id":bob_task["task_id"], "state":"done"});
    assert_eq!(
        call_error(&server, &owner_agent, "update_task", bob_close.clone()).await["code"],
        "space.permission_required"
    );
    tasks::space_member(&server, &person, Method::PATCH, "admin").await;
    assert_eq!(
        call_error(&server, &bob_agent, "update_task", alice_close).await["code"],
        "space.permission_required"
    );
    tasks::space_member(&server, &person, Method::PATCH, "viewer").await;
    assert_eq!(
        call_error(&server, &bob_agent, "update_task", bob_close.clone()).await["code"],
        "space.permission_required"
    );
    tasks::space_member(&server, &person, Method::DELETE, "").await;
    assert_eq!(
        call_error(&server, &bob_agent, "update_task", bob_close).await,
        unknown
    );
    assert_eq!(tasks::read(&server, &person, "TK-1").await, alice_original);
    assert_eq!(tasks::read(&server, &person, "TK-2").await, bob_original);
    server.stop().await;
}

async fn update_and_reopen(
    server: &Server,
    person: &Value,
    agent: &Value,
    task: &Value,
    version: &str,
) {
    let updated = call_version(server, agent, "update_task", json!({
        "task_id":task["task_id"], "title":"Revised title", "state":"done", "result":"# Findings\nFirst result"
    }), version).await;
    assert_eq!(updated["task_title"], "Revised title");
    assert_eq!(updated["state"], "done");
    assert_eq!(updated["result"], "# Findings\nFirst result");
    assert_eq!(updated["agent_name"], "Continuing agent");
    assert!(updated["ended_at"].as_i64().unwrap() >= updated["started_at"].as_i64().unwrap());
    assert_readback(server, person, &updated).await;
    let error = call_error(
        server,
        agent,
        "add_checkpoint",
        json!({"task_id":task["task_id"], "content":"Refused"}),
    )
    .await;
    assert_eq!(error["code"], "task.state_invalid");
    assert_eq!(
        error["message"],
        "Operation is not allowed while the task is done; claim or reopen it first"
    );
    let reopened = call_version(
        server,
        agent,
        "update_task",
        json!({"task_id":task["task_id"], "state":"working"}),
        version,
    )
    .await;
    assert_eq!(reopened["task_title"], "Revised title");
    assert_eq!(reopened["state"], "working");
    assert!(reopened["ended_at"].is_null());
    assert_eq!(reopened["result"], updated["result"]);
    assert_readback(server, person, &reopened).await;
    replace_and_clear_result(server, person, agent, task, version).await;
    let checkpoint = call(
        server,
        agent,
        "add_checkpoint",
        json!({"task_id":task["task_id"], "content":"After reopening"}),
    )
    .await;
    assert_eq!(checkpoint["task_title"], "Revised title");
    let key = task["key"].as_str().unwrap();
    assert_eq!(
        tasks::scratchpad(server, person, key).await,
        json!([{"time":checkpoint["time"], "agent_name":"Continuing agent", "content":"After reopening"}])
    );
}

async fn replace_and_clear_result(
    server: &Server,
    person: &Value,
    agent: &Value,
    task: &Value,
    version: &str,
) {
    for (index, result) in ["Replacement result", ""].into_iter().enumerate() {
        let updated = call_version(
            server,
            agent,
            "update_task",
            json!({"task_id":task["task_id"], "result":result,"result_version":index + 1}),
            version,
        )
        .await;
        assert_eq!(updated["result"], result);
        assert_readback(server, person, &updated).await;
    }
}

async fn assert_readback(server: &Server, person: &Value, updated: &Value) {
    assert_eq!(updated["task_title"], updated["title"]);
    let mut expected = updated.clone();
    expected.as_object_mut().unwrap().remove("task_title");
    assert_eq!(
        tasks::read(server, person, updated["key"].as_str().unwrap()).await,
        expected
    );
}
