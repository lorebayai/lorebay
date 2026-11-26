use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, authorize_for, login, request};
use crate::tasks::helpers::{self as tasks, SPACE, TASKS};

use super::{call, call_error};

#[tokio::test]
async fn claim_task_names_other_party_and_explains_final_states() {
    let (server, person, agent) = tasks::setup().await;
    let bob = login(&server, "bob").await;
    let other = authorize_for(&server, &bob, "Other worker").await;
    tasks::space_member(&server, &person, Method::POST, "contributor").await;
    let task = tasks::create(&server, &agent, SPACE, "Held work").await;
    let error = refused_claim(&server, &other, &task).await;
    assert_eq!(error["code"], "task.held");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("Research agent in another party"));
    assert!(!message.contains("claim"));
    let done_task = tasks::create(&server, &agent, SPACE, "Done work").await;
    call(
        &server,
        &agent,
        "update_task",
        json!({"task_id":done_task["task_id"],"state":"done"}),
    )
    .await;
    for (caller, expected_hint) in [(&agent, "reopen it first"), (&other, "create a new task")] {
        let error = refused_claim(&server, caller, &done_task).await;
        assert_eq!(error["code"], "task.state_invalid");
        assert_eq!(
            error["message"],
            format!("Operation is not allowed while the task is done; {expected_hint}")
        );
        assert_eq!(
            tasks::read(&server, &person, done_task["key"].as_str().unwrap()).await["state"],
            "done"
        );
    }
    for state in ["failed", "canceled"] {
        let task = tasks::create(&server, &agent, SPACE, "Final work").await;
        call(
            &server,
            &agent,
            "update_task",
            json!({"task_id":task["task_id"],"state":state,"state_reason":"Stopped"}),
        )
        .await;
        let error = refused_claim(&server, &agent, &task).await;
        assert_eq!(error["code"], "task.state_invalid");
        let message = error["message"].as_str().unwrap();
        assert!(message.contains(state));
        assert!(message.contains("create a new task"));
        assert!(!message.contains("reopen"));
        assert_eq!(
            tasks::read(&server, &person, task["key"].as_str().unwrap()).await["state"],
            state
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn update_task_repeats_state_without_clearing_request_wait() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Wait for approval").await;
    repeat_state(&server, &agent, &task, "working").await;
    let proposed = call(&server, &agent, "create_request", json!({"task_id":task["task_id"],"wait":true,"action":"create","name":"approved-guide","summary":"Guide","content":"Guide body"})).await;
    repeat_state(&server, &agent, &task, "waiting").await;
    let path = format!(
        "/api/v1/spaces/{SPACE}/requests/{}/approve",
        proposed["key"].as_str().unwrap()
    );
    let approved = request(&server, &person, Method::POST, &path, json!({"revision":1})).await;
    assert_eq!(approved.status(), StatusCode::OK);
    assert_eq!(
        tasks::read(&server, &person, "TK-1").await["state"],
        "working"
    );
    repeat_state(&server, &agent, &task, "working").await;
    complete_deployment_step(&server, &agent, &task).await;
    repeat_state(&server, &agent, &task, "done").await;
    for state in ["failed", "canceled"] {
        let task = tasks::create(&server, &agent, SPACE, "Finish work").await;
        call(
            &server,
            &agent,
            "update_task",
            json!({"task_id":task["task_id"],"state":state,"state_reason":"Stopped"}),
        )
        .await;
        repeat_state(&server, &agent, &task, state).await;
        let refused = call_error(
            &server,
            &agent,
            "update_task",
            json!({"task_id":task["task_id"],"result":"Forbidden change"}),
        )
        .await;
        assert_eq!(refused["code"], "task.state_invalid");
        assert_eq!(
            refused["message"],
            format!("Operation is not allowed while the task is {state}; create a new task")
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn state_invalid_mcp_hints_fit_planned_done_and_final_tasks() {
    let (server, person, agent) = tasks::setup().await;
    let planned = call(
        &server,
        &agent,
        "create_task",
        json!({"space":SPACE, "title":"Planned task", "planned":true}),
    )
    .await;
    assert_checkpoint_hint(
        &server,
        &person,
        &agent,
        &planned,
        "Operation is not allowed while the task is planned; claim it first",
    )
    .await;
    let done = tasks::create(&server, &agent, SPACE, "Done task").await;
    call(
        &server,
        &agent,
        "update_task",
        json!({"task_id":done["task_id"], "state":"done"}),
    )
    .await;
    let cancel_error = call_error(
        &server,
        &agent,
        "update_task",
        json!({"task_id":done["task_id"], "state":"canceled", "state_reason":"Too late"}),
    )
    .await;
    assert_eq!(cancel_error["code"], "task.state_invalid");
    assert_eq!(
        cancel_error["message"],
        "Operation is not allowed while the task is done; a finished task cannot be canceled"
    );
    assert_eq!(
        tasks::read(&server, &person, done["key"].as_str().unwrap()).await["state"],
        "done"
    );
    for state in ["failed", "canceled"] {
        let task = tasks::create(&server, &agent, SPACE, "Final task").await;
        call(
            &server,
            &agent,
            "update_task",
            json!({"task_id":task["task_id"], "state":state, "state_reason":"Stopped"}),
        )
        .await;
        assert_checkpoint_hint(
            &server,
            &person,
            &agent,
            &task,
            &format!("Operation is not allowed while the task is {state}; create a new task"),
        )
        .await;
    }
    server.stop().await;
}

#[tokio::test]
async fn update_task_to_planned_hints_release_before_work_and_end_after_work() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Started work").await;
    let before_work = call_error(
        &server,
        &agent,
        "update_task",
        json!({"task_id":task["task_id"], "state":"planned"}),
    )
    .await;
    assert_eq!(before_work["code"], "task.state_invalid");
    assert_eq!(
        before_work["message"],
        "Operation is not allowed while the task is working; release it instead"
    );
    assert_eq!(
        tasks::read(&server, &person, "TK-1").await["state"],
        "working"
    );
    call(
        &server,
        &agent,
        "add_checkpoint",
        json!({"task_id":task["task_id"], "content":"Recorded work"}),
    )
    .await;
    let after_checkpoint = tasks::read(&server, &person, "TK-1").await;
    let after_work = call_error(
        &server,
        &agent,
        "update_task",
        json!({"task_id":task["task_id"], "state":"planned"}),
    )
    .await;
    assert_eq!(after_work["code"], "task.state_invalid");
    assert_eq!(
        after_work["message"],
        "Operation is not allowed while the task is working; end the task instead because it already has recorded work"
    );
    assert_eq!(
        tasks::read(&server, &person, "TK-1").await,
        after_checkpoint
    );
    let ended = call(
        &server,
        &agent,
        "update_task",
        json!({"task_id":task["task_id"], "state":"done"}),
    )
    .await;
    assert_eq!(ended["state"], "done");
    server.stop().await;
}

async fn assert_checkpoint_hint(
    server: &Server,
    person: &Value,
    agent: &Value,
    task: &Value,
    expected_message: &str,
) {
    let error = call_error(
        server,
        agent,
        "add_checkpoint",
        json!({"task_id":task["task_id"], "content":"Refused checkpoint"}),
    )
    .await;
    assert_eq!(error["code"], "task.state_invalid");
    assert_eq!(error["message"], expected_message);
    assert_eq!(
        tasks::scratchpad(server, person, task["key"].as_str().unwrap()).await,
        json!([])
    );
}

#[tokio::test]
async fn release_task_records_planner_before_cross_party_claim() {
    let (server, person, agent) = tasks::setup().await;
    let peer = authorize_for(&server, &person, "Continuing agent").await;
    let bob = login(&server, "bob").await;
    let other = authorize_for(&server, &bob, "Other worker").await;
    tasks::space_member(&server, &person, Method::POST, "contributor").await;
    let task = tasks::create(&server, &agent, SPACE, "Started work").await;
    assert!(tasks::read(&server, &person, "TK-1").await["planner"].is_null());
    hold(&server, &peer, &task, "claim_task").await;
    let released = hold(&server, &peer, &task, "release_task").await;
    assert_eq!(released["state"], "planned");
    assert_eq!(released["planner"], "alice");
    assert_eq!(released["planner_agent_name"], "Continuing agent");
    let claimed = hold(&server, &other, &task, "claim_task").await;
    assert_eq!(claimed["person"], "bob");
    hold(&server, &other, &task, "release_task").await;
    let after = tasks::read(&server, &person, "TK-1").await;
    assert_eq!(after["state"], "planned");
    assert_eq!(after["person"], "alice");
    assert_eq!(after["planner"], "alice");
    assert_eq!(after["planner_agent_name"], "Continuing agent");
    assert!(after["holder"].is_null());
    server.stop().await;
}

async fn refused_claim(server: &Server, agent: &Value, task: &Value) -> Value {
    call_error(
        server,
        agent,
        "claim_task",
        json!({"task_id":task["task_id"]}),
    )
    .await
}

async fn hold(server: &Server, agent: &Value, task: &Value, tool: &str) -> Value {
    call(server, agent, tool, json!({"task_id":task["task_id"]})).await
}

async fn repeat_state(server: &Server, agent: &Value, task: &Value, state: &str) {
    let key = task["key"].as_str().unwrap();
    let before = tasks::read(server, agent, key).await;
    call(
        server,
        agent,
        "update_task",
        json!({"task_id":task["task_id"],"state":state}),
    )
    .await;
    assert_eq!(tasks::read(server, agent, key).await, before);
}

async fn complete_deployment_step(server: &Server, agent: &Value, task: &Value) {
    let path = format!("{TASKS}/{}", task["task_id"].as_str().unwrap());
    let changed = request(
        server,
        agent,
        Method::PATCH,
        &path,
        json!({"state":"done","result":"Deployment checked"}),
    )
    .await;
    assert_eq!(changed.status(), StatusCode::OK);
    let shown = tasks::read(server, agent, task["key"].as_str().unwrap()).await;
    assert_eq!(shown["state"], "done");
    assert_eq!(shown["result"], "Deployment checked");
}
