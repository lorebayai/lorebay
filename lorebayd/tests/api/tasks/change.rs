use std::time::Duration;

use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::helpers::{
    Server, TEST_PASSWORD, assert_problem, authorize_for, create_space, list_page, login, request,
};

use super::helpers::{
    SPACE, TASKS, append, checkpoint_request, create, read, scratchpad, setup, space_member,
};

#[tokio::test]
async fn change_task_sets_end_time_and_persists_done() {
    let (server, person, agent) = setup().await;
    let created = create(&server, &agent, SPACE, "Original").await;
    let original = read(&server, &person, "TK-1").await;
    change(
        &server,
        &agent,
        &created,
        json!({"title":"Revised #KN-999"}),
    )
    .await;
    let done = change(&server, &agent, &created, json!({"state":"done"})).await;
    assert!(done["ended_at"].as_i64().unwrap() >= original["started_at"].as_i64().unwrap());
    assert_eq!(done["state"], "done");
    assert_eq!(done["title"], "Revised #KN-999");
    assert!(done["holder"].is_null());
    let directory = server.stop().await;
    let server = Server::restart_with_users(
        directory,
        &[("alice", TEST_PASSWORD), ("bob", TEST_PASSWORD)],
    )
    .await;
    assert_eq!(read(&server, &person, "TK-1").await, done);
    server.stop().await;
}

#[tokio::test]
async fn change_task_clears_end_time_when_state_returns_to_working() {
    let (server, _person, agent) = setup().await;
    let created = create(&server, &agent, SPACE, "Resumable").await;
    let ended = change(&server, &agent, &created, json!({"state":"done"})).await;
    let mut expected = ended.clone();
    expected["state"] = json!("working");
    expected["ended_at"] = Value::Null;
    expected["holder"] = json!("Research agent");
    let reopened = change(&server, &agent, &created, json!({"state":"working"})).await;
    expected["state_changed_at"] = reopened["state_changed_at"].clone();
    assert_eq!(reopened, expected);
    sleep(Duration::from_millis(2)).await;
    let ended_again = change(&server, &agent, &created, json!({"state":"done"})).await;
    assert!(ended_again["ended_at"].as_i64().unwrap() > ended["ended_at"].as_i64().unwrap());
    expected["state"] = json!("done");
    expected["holder"] = Value::Null;
    expected["ended_at"] = ended_again["ended_at"].clone();
    expected["state_changed_at"] = ended_again["ended_at"].clone();
    assert_eq!(ended_again, expected);
    server.stop().await;
}

#[tokio::test]
async fn change_task_replaces_result_and_updates_last_activity() {
    let (server, person, agent) = setup().await;
    let first = create(&server, &agent, SPACE, "First").await;
    sleep(Duration::from_millis(2)).await;
    let second = create(&server, &agent, SPACE, "Second").await;
    let before = list_page(&server, &person, TASKS, &[]).await;
    assert_eq!(before["data"][0]["key"], second["key"]);
    let original = read(&server, &person, "TK-1").await;
    sleep(Duration::from_millis(2)).await;
    let changed = change(
        &server,
        &agent,
        &first,
        json!({"result":"# Findings\nFirst result with #KN-999"}),
    )
    .await;
    let mut expected = original.clone();
    expected["result"] = json!("# Findings\nFirst result with #KN-999");
    expected["result_version"] = json!(1);
    expected["last_activity_at"] = changed["last_activity_at"].clone();
    assert_eq!(changed, expected);
    assert!(changed["last_activity_at"].as_i64() > before["data"][0]["last_activity_at"].as_i64());
    let after = list_page(&server, &person, TASKS, &[("page_size", "1")]).await;
    assert_eq!(after["data"][0]["key"], first["key"]);
    assert_eq!(
        after["data"][0]["last_activity_at"],
        changed["last_activity_at"]
    );
    let cursor = after["meta"]["next_cursor"].as_str().unwrap();
    let tail = list_page(&server, &person, TASKS, &[("cursor", cursor)]).await;
    assert_eq!(tail["data"][0]["key"], second["key"]);
    assert!(tail["meta"]["next_cursor"].is_null());
    change(&server, &agent, &first, json!({"state":"done"})).await;
    sleep(Duration::from_millis(2)).await;
    let replaced = change(
        &server,
        &agent,
        &first,
        json!({"state":"working", "result":"Replacement Markdown **result**", "result_version":1}),
    )
    .await;
    expected["state"] = json!("working");
    expected["holder"] = json!("Research agent");
    expected["ended_at"] = Value::Null;
    expected["result"] = json!("Replacement Markdown **result**");
    expected["result_version"] = json!(2);
    expected["last_activity_at"] = replaced["last_activity_at"].clone();
    expected["state_changed_at"] = replaced["state_changed_at"].clone();
    assert_eq!(replaced, expected);
    assert!(replaced["last_activity_at"].as_i64() > changed["last_activity_at"].as_i64());
    clear_result(&server, &agent, &first, expected).await;
    server.stop().await;
}

#[tokio::test]
async fn add_checkpoint_fails_with_reopen_hint_when_task_is_done() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Paused work").await;
    append(&server, &agent, &task, "Before ending").await;
    let checkpoints = scratchpad(&server, &person, "TK-1").await;
    let ended = change(&server, &agent, &task, json!({"state":"done"})).await;
    let refusal = assert_problem(
        checkpoint_request(
            &server,
            &agent,
            SPACE,
            task["task_id"].as_str().unwrap(),
            "Rejected checkpoint",
        )
        .await,
        StatusCode::CONFLICT,
        "task.state_invalid",
    )
    .await;
    assert_eq!(
        refusal["title"],
        "Operation is not allowed while the task is done; reopen it first"
    );
    assert_eq!(scratchpad(&server, &person, "TK-1").await, checkpoints);
    assert_eq!(read(&server, &person, "TK-1").await, ended);
    change(&server, &agent, &task, json!({"state":"working"})).await;
    let added = append(&server, &agent, &task, "After reopening").await;
    assert_eq!(added["task_title"], "Paused work");
    let checkpoints = scratchpad(&server, &person, "TK-1").await;
    assert_eq!(checkpoints.as_array().unwrap().len(), 2);
    assert_eq!(
        checkpoints[1],
        json!({"time":added["time"], "agent_name":"Research agent","content":"After reopening"})
    );
    let reopened = read(&server, &person, "TK-1").await;
    assert_eq!(reopened["last_activity_at"], added["time"]);
    assert_eq!(reopened["state"], "working");
    assert!(reopened["ended_at"].is_null());
    server.stop().await;
}

#[tokio::test]
async fn cancel_task_fails_without_claim_or_reopen_hint_when_task_is_done() {
    let (server, person, _) = setup().await;
    let bob = login(&server, "bob").await;
    let bob_agent = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    let bob_task = create(&server, &bob_agent, SPACE, "Bob finished task").await;
    let done = change(&server, &bob_agent, &bob_task, json!({"state":"done"})).await;
    space_member(&server, &person, Method::DELETE, "").await;
    let refusal = assert_problem(
        change_request(
            &server,
            &person,
            SPACE,
            bob_task["task_id"].as_str().unwrap(),
            json!({"state":"canceled", "state_reason":"No longer needed"}),
        )
        .await,
        StatusCode::CONFLICT,
        "task.state_invalid",
    )
    .await;
    assert_eq!(
        refusal["title"],
        "Operation is not allowed while the task is done; a finished task cannot be canceled"
    );
    assert_eq!(read(&server, &person, "TK-1").await, done);
    server.stop().await;
}

#[tokio::test]
async fn add_checkpoint_fails_with_state_hint_for_planned_and_final_tasks() {
    let (server, person, agent) = setup().await;
    let response = request(
        &server,
        &person,
        Method::POST,
        TASKS,
        json!({"title":"Planned work", "planned":true}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let planned = response.json::<Value>().await.unwrap()["data"].clone();
    assert_checkpoint_refused(
        &server,
        &person,
        &agent,
        &planned,
        "Operation is not allowed while the task is planned; claim it first",
    )
    .await;
    assert_eq!(read(&server, &person, "TK-1").await["state"], "planned");
    for state in ["failed", "canceled"] {
        assert_final_task_writes_refused(&server, &person, &agent, state).await;
    }
    server.stop().await;
}

#[tokio::test]
async fn change_task_to_planned_hints_release_before_work_and_end_after_work() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Started work").await;
    let id = task["task_id"].as_str().unwrap();
    let before_work = assert_problem(
        change_request(&server, &agent, SPACE, id, json!({"state":"planned"})).await,
        StatusCode::CONFLICT,
        "task.state_invalid",
    )
    .await;
    assert_eq!(
        before_work["title"],
        "Operation is not allowed while the task is working; release it instead"
    );
    assert_eq!(read(&server, &person, "TK-1").await["state"], "working");
    append(&server, &agent, &task, "Recorded work").await;
    let after_checkpoint = read(&server, &person, "TK-1").await;
    let after_work = assert_problem(
        change_request(&server, &agent, SPACE, id, json!({"state":"planned"})).await,
        StatusCode::CONFLICT,
        "task.state_invalid",
    )
    .await;
    assert_eq!(
        after_work["title"],
        "Operation is not allowed while the task is working; end the task instead because it already has recorded work"
    );
    assert_eq!(read(&server, &person, "TK-1").await, after_checkpoint);
    change(&server, &agent, &task, json!({"state":"done"})).await;
    server.stop().await;
}

async fn assert_checkpoint_refused(
    server: &Server,
    person: &Value,
    agent: &Value,
    task: &Value,
    expected_title: &str,
) {
    let refusal = assert_problem(
        checkpoint_request(
            server,
            agent,
            SPACE,
            task["task_id"].as_str().unwrap(),
            "Refused checkpoint",
        )
        .await,
        StatusCode::CONFLICT,
        "task.state_invalid",
    )
    .await;
    assert_eq!(refusal["title"], expected_title);
    assert_eq!(
        scratchpad(server, person, task["key"].as_str().unwrap()).await,
        json!([])
    );
}

async fn assert_final_task_writes_refused(
    server: &Server,
    person: &Value,
    agent: &Value,
    state: &str,
) {
    let task = create(server, agent, SPACE, "Final work").await;
    let final_task = change(
        server,
        agent,
        &task,
        json!({"state":state, "state_reason":"Stopped"}),
    )
    .await;
    let expected = format!("Operation is not allowed while the task is {state}; create a new task");
    assert_checkpoint_refused(server, person, agent, &task, &expected).await;
    let update_refusal = assert_problem(
        change_request(
            server,
            agent,
            SPACE,
            task["task_id"].as_str().unwrap(),
            json!({"result":"Forbidden change"}),
        )
        .await,
        StatusCode::CONFLICT,
        "task.state_invalid",
    )
    .await;
    assert_eq!(update_refusal["title"], expected);
    assert_eq!(
        read(server, person, task["key"].as_str().unwrap()).await,
        final_task
    );
}

#[tokio::test]
async fn change_task_fails_when_task_belongs_to_another_party() {
    let (server, person, owner_agent) = setup().await;
    let bob = login(&server, "bob").await;
    let bob_agent = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    let alice_task = create(&server, &owner_agent, SPACE, "Alice task").await;
    let bob_task = create(&server, &bob_agent, SPACE, "Bob task").await;
    for tokens in [&bob, &bob_agent] {
        for changes in [
            json!({"title":"Forbidden"}),
            json!({"state":"done"}),
            json!({"state":"canceled", "state_reason":"Forbidden"}),
            json!({"result":"Forbidden"}),
        ] {
            assert_change_denied(&server, tokens, &alice_task, changes).await;
        }
    }
    for role in ["contributor", "admin"] {
        space_member(&server, &person, Method::PATCH, role).await;
        assert_change_denied(&server, &bob_agent, &alice_task, json!({"state":"done"})).await;
    }
    assert_change_denied(&server, &owner_agent, &bob_task, json!({"state":"done"})).await;
    space_member(&server, &person, Method::DELETE, "").await;
    create_space(&server, &bob, "Bob only", "Bob work").await;
    for tokens in [&bob, &bob_agent] {
        for task in [&alice_task, &bob_task] {
            assert_task_hidden(&server, tokens, task).await;
        }
    }
    assert_eq!(read(&server, &person, "TK-1").await["state"], "working");
    assert_eq!(read(&server, &person, "TK-2").await["state"], "working");
    server.stop().await;
}

#[tokio::test]
async fn change_task_fails_when_path_names_task_by_key() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Original").await;
    let original = read(&server, &person, "TK-1").await;
    create_space(&server, &person, "Other", "Other work").await;
    let id = task["task_id"].as_str().unwrap();
    let uppercase = id.to_uppercase();
    let mut expected = None;
    for tokens in [&person, &agent] {
        for (space, id) in [
            (SPACE, "TK-1"),
            (SPACE, "unknown"),
            (SPACE, uppercase.as_str()),
            ("SPACE-2", id),
        ] {
            let error = assert_problem(
                change_request(&server, tokens, space, id, json!({"state":"done"})).await,
                StatusCode::NOT_FOUND,
                "task.not_found",
            )
            .await;
            if let Some(expected) = &expected {
                assert_eq!(&error, expected);
            } else {
                expected = Some(error);
            }
        }
    }
    assert_eq!(read(&server, &person, "TK-1").await, original);
    server.stop().await;
}

#[tokio::test]
async fn change_task_cancels_another_persons_task_when_caller_is_admin() {
    let (server, person, agent) = setup().await;
    let bob = login(&server, "bob").await;
    let bob_agent = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    space_member(&server, &person, Method::PATCH, "admin").await;
    let alice_task = create(&server, &agent, SPACE, "Alice task").await;
    let bob_task = create(&server, &bob_agent, SPACE, "Bob task").await;
    let alice_original = read(&server, &person, "TK-1").await;
    let bob_original = read(&server, &person, "TK-2").await;
    let ended = change(
        &server,
        &bob,
        &alice_task,
        json!({"state":"canceled", "state_reason":"Stopped by admin"}),
    )
    .await;
    let mut expected = alice_original;
    expected["state_reason"] = json!("Stopped by admin");
    expected["state"] = json!("canceled");
    expected["holder"] = Value::Null;
    expected["ended_at"] = ended["ended_at"].clone();
    expected["state_changed_by"] = json!("bob");
    expected["state_changed_agent_name"] = Value::Null;
    expected["state_changed_at"] = ended["ended_at"].clone();
    assert!(ended["ended_at"].is_i64());
    assert_eq!(ended, expected);
    space_member(&server, &person, Method::DELETE, "").await;
    let ended = change(
        &server,
        &person,
        &bob_task,
        json!({"state":"canceled", "state_reason":"Stopped by owner"}),
    )
    .await;
    let mut expected = bob_original;
    expected["state_reason"] = json!("Stopped by owner");
    expected["state"] = json!("canceled");
    expected["holder"] = Value::Null;
    expected["ended_at"] = ended["ended_at"].clone();
    expected["state_changed_by"] = json!("alice");
    expected["state_changed_agent_name"] = Value::Null;
    expected["state_changed_at"] = ended["ended_at"].clone();
    assert!(ended["ended_at"].is_i64());
    assert_eq!(ended, expected);
    server.stop().await;
}

#[tokio::test]
async fn change_task_fails_when_admin_edits_another_persons_title() {
    let (server, person, agent) = setup().await;
    let bob = login(&server, "bob").await;
    let bob_agent = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    space_member(&server, &person, Method::PATCH, "admin").await;
    let alice_task = create(&server, &agent, SPACE, "Original").await;
    let bob_task = create(&server, &bob_agent, SPACE, "Original").await;
    for (tokens, task) in [(&bob, &alice_task), (&person, &bob_task)] {
        for changes in [
            json!({"title":"Forbidden"}),
            json!({"result":"Forbidden"}),
            json!({"state":"working"}),
            json!({"state":"done","title":"Original"}),
            json!({"state":"done","result":""}),
            json!({"state":"done","title":"Forbidden","result":"Forbidden"}),
        ] {
            assert_change_denied(&server, tokens, task, changes).await;
        }
        change(
            &server,
            tokens,
            task,
            json!({"state":"canceled", "state_reason":"Stopped"}),
        )
        .await;
        for changes in [
            json!({"state":"working"}),
            json!({"title":"Forbidden"}),
            json!({"result":"Forbidden"}),
        ] {
            assert_change_denied(&server, tokens, task, changes).await;
        }
    }
    server.stop().await;
}

#[tokio::test]
async fn change_task_fails_when_person_became_viewer() {
    let (server, person, _) = setup().await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    let task = create(&server, &agent, SPACE, "Bob task").await;
    change(&server, &agent, &task, json!({"result":"Existing result"})).await;
    space_member(&server, &person, Method::PATCH, "viewer").await;
    for tokens in [&bob, &agent] {
        for changes in [
            json!({"title":"Forbidden"}),
            json!({"state":"done"}),
            json!({"result":""}),
        ] {
            assert_change_denied(&server, tokens, &task, changes).await;
        }
    }
    server.stop().await;
}

#[tokio::test]
async fn change_task_fails_when_no_field_is_given() {
    let server = Server::executable().await;
    let person = server.tokens().await;
    create_space(&server, &person, "Team", "Shared work").await;
    let agent = authorize_for(&server, &person, "Research agent").await;
    let task = create(&server, &agent, SPACE, "Original").await;
    let original = read(&server, &person, "TK-1").await;
    for (body, field, code) in [
        (json!({}), "body", "required"),
        (json!({"state":"unknown"}), "state", "invalid"),
        (json!({"title":""}), "title", "required"),
        (json!({"title":"  "}), "title", "required"),
        (json!({"title":"x".repeat(201)}), "title", "too_long"),
        (json!({"title":"\u{4e2d}".repeat(201)}), "title", "too_long"),
        (json!({"result":"x".repeat(100_001)}), "result", "too_long"),
        (
            json!({"title":"Uncommitted","state":"done","result":"\u{4e2d}".repeat(100_001)}),
            "result",
            "too_long",
        ),
        (json!({"title":null}), "title", "invalid"),
        (json!({"state":null}), "state", "invalid"),
        (json!({"result":null}), "result", "invalid"),
    ] {
        let error = assert_problem(
            change_request(
                &server,
                &agent,
                SPACE,
                task["task_id"].as_str().unwrap(),
                body,
            )
            .await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
        assert_eq!(error["errors"][0]["field"], field);
        assert_eq!(error["errors"][0]["code"], code);
        assert_eq!(read(&server, &person, "TK-1").await, original);
    }
    assert_task_text_limits(&server, &agent, &task).await;
    server.stop().await;
}

async fn change_request(
    server: &Server,
    tokens: &Value,
    space: &str,
    id: &str,
    changes: Value,
) -> Response {
    request(
        server,
        tokens,
        Method::PATCH,
        &format!("/api/v1/spaces/{space}/tasks/{id}"),
        changes,
    )
    .await
}

async fn change(server: &Server, tokens: &Value, task: &Value, changes: Value) -> Value {
    let response = change_request(
        server,
        tokens,
        SPACE,
        task["task_id"].as_str().unwrap(),
        changes,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(body["meta"], json!({}));
    let changed = body["data"].clone();
    assert_eq!(
        read(server, tokens, task["key"].as_str().unwrap()).await,
        changed
    );
    changed
}

async fn assert_change_denied(server: &Server, tokens: &Value, task: &Value, changes: Value) {
    let original = read(server, tokens, task["key"].as_str().unwrap()).await;
    let problem = assert_problem(
        change_request(
            server,
            tokens,
            SPACE,
            task["task_id"].as_str().unwrap(),
            changes,
        )
        .await,
        StatusCode::FORBIDDEN,
        "space.permission_required",
    )
    .await;
    assert_eq!(problem["errors"], json!([]));
    assert_eq!(
        read(server, tokens, task["key"].as_str().unwrap()).await,
        original
    );
}

async fn assert_task_hidden(server: &Server, tokens: &Value, task: &Value) {
    for (path_space, missing_space, code) in [
        ("SPACE-2", "SPACE-2", "task.not_found"),
        (SPACE, "SPACE-999", "space.not_found"),
    ] {
        let missing = assert_problem(
            change_request(
                server,
                tokens,
                missing_space,
                "unknown",
                json!({"state":"done"}),
            )
            .await,
            StatusCode::NOT_FOUND,
            code,
        )
        .await;
        assert_eq!(
            assert_problem(
                change_request(
                    server,
                    tokens,
                    path_space,
                    task["task_id"].as_str().unwrap(),
                    json!({"state":"done"})
                )
                .await,
                StatusCode::NOT_FOUND,
                code,
            )
            .await,
            missing
        );
    }
}

async fn assert_task_text_limits(server: &Server, agent: &Value, task: &Value) {
    let title = "\u{4e2d}".repeat(200);
    let result = "\u{4e2d}".repeat(100_000);
    let changed = change(
        server,
        agent,
        task,
        json!({"title":title,"result":result,"state":"done"}),
    )
    .await;
    assert_eq!(changed["title"], title);
    assert_eq!(changed["result"], result);
    assert_eq!(changed["state"], "done");
    assert!(changed["ended_at"].is_i64());
    assert_eq!(
        change(
            server,
            agent,
            task,
            json!({"state":"working", "result":"  ", "result_version":1})
        )
        .await["result"],
        "  "
    );
    assert_eq!(
        change(server, agent, task, json!({"result":"","result_version":2})).await["result"],
        ""
    );
}

async fn clear_result(server: &Server, agent: &Value, task: &Value, mut expected: Value) {
    let cleared = change(server, agent, task, json!({"result":"","result_version":2})).await;
    expected["result"] = json!("");
    expected["result_version"] = json!(3);
    expected["last_activity_at"] = cleared["last_activity_at"].clone();
    assert_eq!(cleared, expected);
    assert_eq!(scratchpad(server, agent, "TK-1").await, json!([]));
}
