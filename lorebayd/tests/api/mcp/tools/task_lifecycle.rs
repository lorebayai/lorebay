use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::agent_files::helpers::upload_ok;
use crate::helpers::{Server, assert_problem, authorize_for, login, request};
use crate::tasks::helpers::{self as tasks, SPACE, TASKS};

use super::{call, call_error};

#[tokio::test]
async fn update_task_follows_allowed_transitions_and_keeps_final_states() {
    let (server, person, agent) = tasks::setup().await;
    for final_state in ["done", "failed", "canceled"] {
        let task = tasks::create(&server, &agent, SPACE, "Work through outcome").await;
        let mut changed = transition(
            &server,
            &agent,
            &task,
            "waiting",
            Some("Awaiting an answer"),
        )
        .await;
        assert_eq!(changed["state_reason"], "Awaiting an answer");
        changed = transition(&server, &agent, &task, "working", None).await;
        assert!(changed["state_reason"].is_null());
        let reason = (final_state != "done").then_some("Work stopped");
        changed = transition(&server, &agent, &task, final_state, reason).await;
        assert!(changed["holder"].is_null());
        assert!(changed["ended_at"].is_i64());
        if final_state == "done" {
            changed = transition(&server, &agent, &task, "working", None).await;
            assert_eq!(changed["holder"], "Research agent");
            assert!(changed["ended_at"].is_null());
            transition(&server, &agent, &task, "waiting", Some("Awaiting review")).await;
            changed = transition(&server, &agent, &task, "done", None).await;
        }
        let error = call_error(
            &server,
            &agent,
            "update_task",
            json!({"task_id":task["task_id"],"state":"planned"}),
        )
        .await;
        assert_eq!(error["code"], "task.state_invalid");
        assert_eq!(
            tasks::read(&server, &person, task["key"].as_str().unwrap()).await["state"],
            changed["state"]
        );
    }
    for state in ["failed", "canceled"] {
        let task = tasks::create(&server, &agent, SPACE, "Wait then stop").await;
        transition(&server, &agent, &task, "waiting", Some("Pending answer")).await;
        transition(&server, &agent, &task, state, Some("Cannot continue")).await;
        let error = call_error(
            &server,
            &agent,
            "update_task",
            json!({"task_id":task["task_id"],"state":"working"}),
        )
        .await;
        assert_eq!(error["code"], "task.state_invalid");
    }
    server.stop().await;
}

#[tokio::test]
async fn update_task_requires_one_line_reason_before_waiting_or_stopping() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Wait for a person").await;
    for state in ["waiting", "failed", "canceled"] {
        for reason in [
            None,
            Some(""),
            Some("  "),
            Some("First\nSecond"),
            Some("First\rSecond"),
        ] {
            let mut body = json!({"task_id":task["task_id"], "state":state});
            if let Some(reason) = reason {
                body["state_reason"] = json!(reason);
            }
            assert_eq!(
                call_error(&server, &agent, "update_task", body).await["code"],
                "task.reason_invalid"
            );
        }
    }
    let id = task["task_id"].as_str().unwrap();
    assert_problem(
        request(
            &server,
            &person,
            Method::PATCH,
            &format!("{TASKS}/{id}"),
            json!({"state":"done"}),
        )
        .await,
        StatusCode::FORBIDDEN,
        "space.permission_required",
    )
    .await;
    assert_eq!(
        tasks::read(&server, &person, "TK-1").await["state"],
        "working"
    );
    transition(
        &server,
        &agent,
        &task,
        "waiting",
        Some("Awaiting the person's answer"),
    )
    .await;
    let open = call(
        &server,
        &agent,
        "list_tasks",
        json!({"space":SPACE,"own_open":true}),
    )
    .await;
    assert_eq!(open["data"][0]["state"], "waiting");
    server.stop().await;
}

#[tokio::test]
async fn claim_task_moves_planned_task_and_release_restores_planner() {
    let (server, person, agent) = tasks::setup().await;
    let bob = login(&server, "bob").await;
    let other = authorize_for(&server, &bob, "Bob worker").await;
    tasks::space_member(&server, &person, Method::POST, "contributor").await;
    let response = request(
        &server,
        &person,
        Method::POST,
        TASKS,
        json!({"title":"Queued by a person","planned":true}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let task = response.json::<Value>().await.unwrap()["data"].clone();
    let before = tasks::read(&server, &person, "TK-1").await;
    assert_attribution(&before, "alice", None);
    assert_eq!(before["state_changed_at"], before["created_at"]);
    assert_eq!(before["state"], "planned");
    assert_eq!(before["planner"], "alice");
    assert!(before["holder"].is_null());
    assert!(before["started_at"].is_null());
    let claimed = hold(&server, &other, &task, "claim_task").await;
    assert_attribution(&claimed, "bob", Some("Bob worker"));
    assert_eq!(claimed["state_changed_at"], claimed["started_at"]);
    assert_eq!(claimed["person"], "bob");
    assert_eq!(claimed["holder"], "Bob worker");
    assert_eq!(claimed["planner"], "alice");
    let refused = refused_hold(&server, &agent, &task, "claim_task").await;
    assert_eq!(refused["code"], "task.held");
    assert!(refused["message"].as_str().unwrap().contains("Bob worker"));
    let released = hold(&server, &other, &task, "release_task").await;
    assert_attribution(&released, "bob", Some("Bob worker"));
    assert_eq!(released["state"], "planned");
    assert_eq!(released["person"], "alice");
    assert!(released["holder"].is_null());
    hold(&server, &agent, &task, "claim_task").await;
    call(
        &server,
        &agent,
        "add_checkpoint",
        json!({"task_id":task["task_id"],"content":"Started work"}),
    )
    .await;
    assert_eq!(
        refused_hold(&server, &agent, &task, "release_task").await["code"],
        "task.has_work"
    );
    server.stop().await;
}

#[tokio::test]
async fn claim_task_takes_over_and_old_holder_completes_upload() {
    let (server, person, agent) = tasks::setup().await;
    let peer = authorize_for(&server, &person, "Continuing agent").await;
    let task = call(
        &server,
        &agent,
        "create_task",
        json!({"space":SPACE,"title":"Agent's queue","planned":true}),
    )
    .await;
    let planned = tasks::read(&server, &person, "TK-1").await;
    assert_eq!(planned["planner_agent_name"], "Research agent");
    hold(&server, &agent, &task, "claim_task").await;
    assert_non_holder_refusals(&server, &peer, &task).await;
    let file = call(
        &server,
        &agent,
        "create_file",
        json!({"task_id":task["task_id"],"file_name":"report.txt","summary":"Report"}),
    )
    .await;
    assert_eq!(
        refused_hold(&server, &agent, &task, "release_task").await["code"],
        "task.has_work"
    );
    transition(
        &server,
        &agent,
        &task,
        "waiting",
        Some("Awaiting an answer"),
    )
    .await;
    let held = hold(&server, &peer, &task, "claim_task").await;
    assert_eq!(held["state"], "waiting");
    transition(&server, &peer, &task, "working", None).await;
    assert_eq!(held["holder"], "Continuing agent");
    upload_ok(&server, &file, "Report bytes").await;
    let complete = call(
        &server,
        &agent,
        "complete_file",
        json!({"task_id":task["task_id"],"key":file["key"]}),
    )
    .await;
    assert_eq!(complete["size"], 12);
    assert_eq!(
        tasks::read(&server, &person, "TK-1").await["files"][0]["key"],
        file["key"]
    );
    assert_result_prevents_release(&server, &peer, &task).await;
    server.stop().await;
}

#[tokio::test]
async fn create_request_waits_until_approved_or_withdrawn() {
    let (server, person, agent) = tasks::setup().await;
    let bob = login(&server, "bob").await;
    tasks::space_member(&server, &person, Method::POST, "contributor").await;
    for (index, decision) in ["approve", "reject", "reject"].into_iter().enumerate() {
        let task = tasks::create(&server, &agent, SPACE, "Wait for Knowledge decision").await;
        let proposed = call(&server, &agent, "create_request", json!({"task_id":task["task_id"],"wait":true,"action":"create","name":format!("guide-{index}"),"summary":"Guide","content":"Guide body"})).await;
        let waiting = tasks::read(&server, &person, task["key"].as_str().unwrap()).await;
        assert_eq!(waiting["state"], "waiting");
        assert_attribution(&waiting, "alice", Some("Research agent"));
        assert!(
            waiting["state_reason"]
                .as_str()
                .unwrap()
                .contains(proposed["key"].as_str().unwrap())
        );
        if index == 2 {
            transition(&server, &agent, &task, "done", None).await;
        }
        let path = format!(
            "/api/v1/spaces/{SPACE}/requests/{}/{decision}",
            proposed["key"].as_str().unwrap()
        );
        let body = if decision == "reject" {
            json!({"reason":"Withdrawn by sponsor","revision":1})
        } else {
            json!({"revision":1})
        };
        let decider = if index == 0 { &bob } else { &person };
        let response = request(&server, decider, Method::POST, &path, body).await;
        assert_eq!(response.status(), StatusCode::OK);
        let after = tasks::read(&server, &person, task["key"].as_str().unwrap()).await;
        assert_eq!(after["state"], if index == 2 { "done" } else { "working" });
        assert_attribution(
            &after,
            if index == 0 { "bob" } else { "alice" },
            (index == 2).then_some("Research agent"),
        );
        assert!(after["state_changed_at"].as_i64() >= waiting["state_changed_at"].as_i64());
        assert!(after["state_reason"].is_null());
    }
    server.stop().await;
}

async fn transition(
    server: &Server,
    agent: &Value,
    task: &Value,
    state: &str,
    reason: Option<&str>,
) -> Value {
    let mut arguments = json!({"task_id":task["task_id"],"state":state});
    if let Some(reason) = reason {
        arguments["state_reason"] = json!(reason);
    }
    let changed = call(server, agent, "update_task", arguments).await;
    assert_eq!(changed["state"], state);
    changed
}

async fn hold(server: &Server, agent: &Value, task: &Value, tool: &str) -> Value {
    call(server, agent, tool, json!({"task_id":task["task_id"]})).await
}

async fn refused_hold(server: &Server, agent: &Value, task: &Value, tool: &str) -> Value {
    call_error(server, agent, tool, json!({"task_id":task["task_id"]})).await
}

async fn assert_non_holder_refusals(server: &Server, peer: &Value, task: &Value) {
    for (tool, extra) in [
        ("add_checkpoint", json!({"content":"Wrong writer"})),
        (
            "create_file",
            json!({"file_name":"wrong.txt","summary":"Wrong writer"}),
        ),
        ("update_task", json!({"result":"Wrong writer"})),
    ] {
        let mut arguments = extra;
        arguments["task_id"] = task["task_id"].clone();
        let error = call_error(server, peer, tool, arguments).await;
        assert_eq!(error["code"], "task.not_holder");
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("Research agent")
        );
    }
}

#[tokio::test]
async fn claim_task_refuses_viewer_and_person_cancels_planned_task() {
    let (server, person, agent) = tasks::setup().await;
    let bob = login(&server, "bob").await;
    let viewer = authorize_for(&server, &bob, "Viewer agent").await;
    tasks::space_member(&server, &person, Method::POST, "viewer").await;
    let task = call(
        &server,
        &agent,
        "create_task",
        json!({"space":SPACE,"title":"Planned work","planned":true}),
    )
    .await;
    assert_eq!(
        refused_hold(&server, &viewer, &task, "claim_task").await["code"],
        "space.permission_required"
    );
    let id = task["task_id"].as_str().unwrap();
    let response = request(
        &server,
        &person,
        Method::PATCH,
        &format!("{TASKS}/{id}"),
        json!({"state":"canceled","state_reason":"No longer needed"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let after = tasks::read(&server, &person, "TK-1").await;
    assert_eq!(after["state"], "canceled");
    assert_eq!(after["state_reason"], "No longer needed");
    assert!(after["holder"].is_null());
    assert!(after["started_at"].is_null());
    server.stop().await;
}

async fn assert_result_prevents_release(server: &Server, peer: &Value, task: &Value) {
    call(
        server,
        peer,
        "update_task",
        json!({"task_id":task["task_id"],"result":""}),
    )
    .await;
    assert_eq!(
        refused_hold(server, peer, task, "release_task").await["code"],
        "task.has_work"
    );
}

fn assert_attribution(task: &Value, person: &str, agent: Option<&str>) {
    assert_eq!(task["state_changed_by"], person);
    assert_eq!(task["state_changed_agent_name"], json!(agent));
    assert!(task["state_changed_at"].as_i64().unwrap() > 0);
}
