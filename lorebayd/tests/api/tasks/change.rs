use std::time::Duration;

use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::helpers::{
    Server, TEST_PASSWORD, assert_problem, authorize_for, create_space, list_page, login, request,
};

use super::helpers::{
    SPACE, TASKS, append, create, note_request, read, scratchpad, setup, space_member,
};

#[tokio::test]
async fn change_task_sets_end_time_when_status_becomes_ended() {
    let (server, person, agent) = setup().await;
    let created = create(&server, &agent, SPACE, "Original").await;
    let other = authorize_for(&server, &person, "Continuing agent").await;
    let original = read(&server, &person, "TK-1").await;
    let mut expected = original.clone();
    expected["title"] = json!("Revised #KN-999");
    assert_eq!(
        change(
            &server,
            &other,
            &created,
            json!({"title":"Revised #KN-999"})
        )
        .await,
        expected
    );
    let ended = change(&server, &person, &created, json!({"status":"ended"})).await;
    assert!(ended["ended_at"].as_i64().unwrap() >= original["started_at"].as_i64().unwrap());
    expected["status"] = json!("ended");
    expected["ended_at"] = ended["ended_at"].clone();
    assert_eq!(ended, expected);
    sleep(Duration::from_millis(2)).await;
    assert_eq!(
        change(&server, &agent, &created, json!({"status":"ended"})).await,
        ended
    );
    expected["title"] = json!("Final title");
    assert_eq!(
        change(&server, &other, &created, json!({"title":"Final title"})).await,
        expected
    );
    let directory = server.stop().await;
    let server = Server::restart_with_users(
        directory,
        &[("alice", TEST_PASSWORD), ("bob", TEST_PASSWORD)],
    )
    .await;
    assert_eq!(read(&server, &person, "TK-1").await, expected);
    server.stop().await;
}

#[tokio::test]
async fn change_task_clears_end_time_when_status_returns_to_in_progress() {
    let (server, person, agent) = setup().await;
    let created = create(&server, &agent, SPACE, "Resumable").await;
    let ended = change(&server, &agent, &created, json!({"status":"ended"})).await;
    let mut expected = ended.clone();
    expected["status"] = json!("in progress");
    expected["ended_at"] = Value::Null;
    assert_eq!(
        change(&server, &person, &created, json!({"status":"in progress"})).await,
        expected
    );
    sleep(Duration::from_millis(2)).await;
    let ended_again = change(&server, &agent, &created, json!({"status":"ended"})).await;
    assert!(ended_again["ended_at"].as_i64().unwrap() > ended["ended_at"].as_i64().unwrap());
    expected["status"] = json!("ended");
    expected["ended_at"] = ended_again["ended_at"].clone();
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
    let ended = change(&server, &person, &first, json!({"status":"ended"})).await;
    sleep(Duration::from_millis(2)).await;
    let replaced = change(
        &server,
        &person,
        &first,
        json!({"result":"Replacement Markdown **result**"}),
    )
    .await;
    expected["status"] = json!("ended");
    expected["ended_at"] = ended["ended_at"].clone();
    expected["result"] = json!("Replacement Markdown **result**");
    expected["last_activity_at"] = replaced["last_activity_at"].clone();
    assert_eq!(replaced, expected);
    assert!(replaced["last_activity_at"].as_i64() > changed["last_activity_at"].as_i64());
    let cleared = change(&server, &agent, &first, json!({"result":""})).await;
    expected["result"] = json!("");
    expected["last_activity_at"] = cleared["last_activity_at"].clone();
    assert_eq!(cleared, expected);
    assert_eq!(scratchpad(&server, &person, "TK-1").await, json!([]));
    server.stop().await;
}

#[tokio::test]
async fn add_note_fails_with_reopen_hint_when_task_is_ended() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Paused work").await;
    append(&server, &agent, &task, "Before ending").await;
    let notes = scratchpad(&server, &person, "TK-1").await;
    let ended = change(&server, &agent, &task, json!({"status":"ended"})).await;
    let refusal = assert_problem(
        note_request(
            &server,
            &agent,
            SPACE,
            task["task_id"].as_str().unwrap(),
            "Rejected note",
        )
        .await,
        StatusCode::CONFLICT,
        "task.ended",
    )
    .await;
    assert_eq!(
        refusal["title"],
        "Set the task back to in progress before appending a note"
    );
    assert_eq!(scratchpad(&server, &person, "TK-1").await, notes);
    assert_eq!(read(&server, &person, "TK-1").await, ended);
    change(&server, &person, &task, json!({"status":"in progress"})).await;
    let added = append(&server, &agent, &task, "After reopening").await;
    assert_eq!(added["task_title"], "Paused work");
    let notes = scratchpad(&server, &person, "TK-1").await;
    assert_eq!(notes.as_array().unwrap().len(), 2);
    assert_eq!(
        notes[1],
        json!({"time":added["time"],"content":"After reopening"})
    );
    let reopened = read(&server, &person, "TK-1").await;
    assert_eq!(reopened["last_activity_at"], added["time"]);
    assert_eq!(reopened["status"], "in progress");
    assert!(reopened["ended_at"].is_null());
    server.stop().await;
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
            json!({"status":"ended"}),
            json!({"result":"Forbidden"}),
        ] {
            assert_change_denied(&server, tokens, &alice_task, changes).await;
        }
    }
    for role in ["contributor", "admin"] {
        space_member(&server, &person, Method::PATCH, role).await;
        assert_change_denied(&server, &bob_agent, &alice_task, json!({"status":"ended"})).await;
    }
    assert_change_denied(&server, &owner_agent, &bob_task, json!({"status":"ended"})).await;
    space_member(&server, &person, Method::DELETE, "").await;
    create_space(&server, &bob, "Bob only", "Bob work").await;
    for tokens in [&bob, &bob_agent] {
        for task in [&alice_task, &bob_task] {
            assert_task_hidden(&server, tokens, task).await;
        }
    }
    assert_eq!(
        read(&server, &person, "TK-1").await["status"],
        "in progress"
    );
    assert_eq!(
        read(&server, &person, "TK-2").await["status"],
        "in progress"
    );
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
                change_request(&server, tokens, space, id, json!({"status":"ended"})).await,
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
async fn change_task_closes_another_persons_task_when_caller_is_admin() {
    let (server, person, agent) = setup().await;
    let bob = login(&server, "bob").await;
    let bob_agent = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    space_member(&server, &person, Method::PATCH, "admin").await;
    let alice_task = create(&server, &agent, SPACE, "Alice task").await;
    let bob_task = create(&server, &bob_agent, SPACE, "Bob task").await;
    let alice_original = read(&server, &person, "TK-1").await;
    let bob_original = read(&server, &person, "TK-2").await;
    let ended = change(&server, &bob, &alice_task, json!({"status":"ended"})).await;
    let mut expected = alice_original;
    expected["status"] = json!("ended");
    expected["ended_at"] = ended["ended_at"].clone();
    assert!(ended["ended_at"].is_i64());
    assert_eq!(ended, expected);
    space_member(&server, &person, Method::DELETE, "").await;
    let ended = change(&server, &person, &bob_task, json!({"status":"ended"})).await;
    let mut expected = bob_original;
    expected["status"] = json!("ended");
    expected["ended_at"] = ended["ended_at"].clone();
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
            json!({"status":"in progress"}),
            json!({"status":"ended","title":"Original"}),
            json!({"status":"ended","result":""}),
            json!({"status":"ended","title":"Forbidden","result":"Forbidden"}),
        ] {
            assert_change_denied(&server, tokens, task, changes).await;
        }
        change(&server, tokens, task, json!({"status":"ended"})).await;
        for changes in [
            json!({"status":"in progress"}),
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
    change(&server, &bob, &task, json!({"result":"Existing result"})).await;
    space_member(&server, &person, Method::PATCH, "viewer").await;
    for tokens in [&bob, &agent] {
        for changes in [
            json!({"title":"Forbidden"}),
            json!({"status":"ended"}),
            json!({"result":""}),
        ] {
            assert_change_denied(&server, tokens, &task, changes).await;
        }
    }
    server.stop().await;
}

#[tokio::test]
async fn change_task_fails_when_no_field_is_given() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Original").await;
    let original = read(&server, &person, "TK-1").await;
    for (body, field, code) in [
        (json!({}), "body", "required"),
        (json!({"status":"unknown"}), "status", "invalid"),
        (json!({"title":""}), "title", "required"),
        (json!({"title":"  "}), "title", "required"),
        (json!({"title":"x".repeat(201)}), "title", "too_long"),
        (json!({"title":"\u{4e2d}".repeat(201)}), "title", "too_long"),
        (json!({"result":"x".repeat(100_001)}), "result", "too_long"),
        (
            json!({"title":"Uncommitted","status":"ended","result":"\u{4e2d}".repeat(100_001)}),
            "result",
            "too_long",
        ),
        (json!({"title":null}), "body", "invalid"),
        (json!({"status":null}), "body", "invalid"),
        (json!({"result":null}), "body", "invalid"),
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
    let title = "\u{4e2d}".repeat(200);
    let result = "\u{4e2d}".repeat(100_000);
    let changed = change(
        &server,
        &agent,
        &task,
        json!({"title":title,"result":result,"status":"ended"}),
    )
    .await;
    assert_eq!(changed["title"], title);
    assert_eq!(changed["result"], result);
    assert_eq!(changed["status"], "ended");
    assert!(changed["ended_at"].is_i64());
    assert_eq!(
        change(&server, &person, &task, json!({"result":"  "})).await["result"],
        "  "
    );
    assert_eq!(
        change(&server, &person, &task, json!({"result":""})).await["result"],
        ""
    );
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
    assert_problem(
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
                json!({"status":"ended"}),
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
                    json!({"status":"ended"})
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
