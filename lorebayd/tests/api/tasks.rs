use std::collections::HashSet;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use tokio::task::JoinSet;

use crate::helpers::{
    Server, TEST_PASSWORD, assert_problem, authorize_for, create_space, list_page, list_response,
    login, request, walk,
};

use helpers::*;

pub(crate) mod helpers;

mod change;
mod deletion;
mod filters;

#[tokio::test]
async fn create_task_returns_task_id_and_key() {
    let (server, person, agent) = setup().await;
    let created = create(&server, &agent, SPACE, "Research #KN-999").await;
    let id = created["task_id"].as_str().unwrap();
    assert_eq!(id.len(), 26);
    assert!(
        id.bytes()
            .all(|byte| b"0123456789abcdefghjkmnpqrstvwxyz".contains(&byte))
    );
    assert_eq!(created["key"], "TK-1");
    let task = read(&server, &person, "TK-1").await;
    assert_eq!(task["title"], "Research #KN-999");
    assert_eq!(task["status"], "in progress");
    assert_eq!(task["person"], "alice");
    assert_eq!(task["agent_name"], "Research agent");
    assert_eq!(task["task_id"], id);
    assert_eq!(task["started_at"], task["last_activity_at"]);
    assert!(task["started_at"].as_i64().unwrap() > 0);
    assert!(task["ended_at"].is_null());
    assert_eq!(task["result"], "");
    assert_eq!(task["files"], json!([]));
    assert!(task.get("scratchpad").is_none());
    assert_eq!(read(&server, &agent, "TK-1").await, task);
    assert_eq!(scratchpad(&server, &person, "TK-1").await, json!([]));
    server.stop().await;
}

#[tokio::test]
async fn create_task_takes_next_key_in_space() {
    let (server, person, agent) = setup().await;
    create_space(&server, &person, "Other", "Other work").await;
    assert_eq!(create(&server, &agent, SPACE, "First").await["key"], "TK-1");
    let (second, third) = tokio::join!(
        create(&server, &agent, SPACE, "Second"),
        create(&server, &agent, SPACE, "Third")
    );
    let mut keys = [
        second["key"].as_str().unwrap(),
        third["key"].as_str().unwrap(),
    ];
    keys.sort_unstable();
    assert_eq!(keys, ["TK-2", "TK-3"]);
    assert_ne!(second["task_id"], third["task_id"]);
    let other = create(&server, &agent, "SPACE-2", "Other first").await;
    assert_eq!(other["key"], "TK-1");
    let directory = server.stop().await;
    let server = Server::restart_with_users(
        directory,
        &[("alice", TEST_PASSWORD), ("bob", TEST_PASSWORD)],
    )
    .await;
    assert_eq!(
        create(&server, &agent, SPACE, "After restart").await["key"],
        "TK-4"
    );
    assert_eq!(
        create(&server, &agent, "SPACE-2", "Other second").await["key"],
        "TK-2"
    );
    server.stop().await;
}

#[tokio::test]
async fn create_task_fails_when_caller_is_person() {
    let (server, person, agent) = setup().await;
    let bob = login(&server, "bob").await;
    let task = create(&server, &agent, SPACE, "Original").await;
    let mut expected = None;
    for tokens in [&person, &bob] {
        for space in [SPACE, "SPACE-999"] {
            let response = request(
                &server,
                tokens,
                Method::POST,
                &format!("/api/v1/spaces/{space}/tasks"),
                json!({"title":"Forbidden"}),
            )
            .await;
            let refusal =
                assert_problem(response, StatusCode::FORBIDDEN, "auth.agent_required").await;
            if let Some(expected) = &expected {
                assert_eq!(&refusal, expected);
            } else {
                expected = Some(refusal.clone());
            }
            assert_eq!(
                assert_problem(
                    checkpoint_request(
                        &server,
                        tokens,
                        space,
                        task["task_id"].as_str().unwrap(),
                        "Forbidden"
                    )
                    .await,
                    StatusCode::FORBIDDEN,
                    "auth.agent_required"
                )
                .await,
                refusal
            );
        }
    }
    assert_eq!(scratchpad(&server, &person, "TK-1").await, json!([]));
    assert_eq!(
        list_page(&server, &person, TASKS, &[]).await["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    server.stop().await;
}

#[tokio::test]
async fn create_task_fails_when_title_is_empty() {
    let (server, person, agent) = setup().await;
    for body in [
        json!({"title":""}),
        json!({"title":"  "}),
        json!({"title":"x".repeat(201)}),
        json!({"title":"\u{4e2d}".repeat(201)}),
        json!({}),
        json!({"title":null}),
    ] {
        let error = assert_problem(
            request(&server, &agent, Method::POST, TASKS, body).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
        assert!(!error["errors"].as_array().unwrap().is_empty());
    }
    assert_eq!(
        list_page(&server, &person, TASKS, &[]).await["data"],
        json!([])
    );
    let title = "\u{4e2d}".repeat(200);
    let task = create(&server, &agent, SPACE, &title).await;
    assert_eq!(task["key"], "TK-1");
    for content in [
        String::new(),
        "  ".to_owned(),
        "x".repeat(10_001),
        "\u{4e2d}".repeat(10_001),
    ] {
        let error = assert_problem(
            checkpoint_request(
                &server,
                &agent,
                SPACE,
                task["task_id"].as_str().unwrap(),
                &content,
            )
            .await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
        assert_eq!(error["errors"][0]["field"], "content");
    }
    assert_eq!(scratchpad(&server, &person, "TK-1").await, json!([]));
    let content = "\u{4e2d}".repeat(10_000);
    append(&server, &agent, &task, &content).await;
    assert_eq!(read(&server, &person, "TK-1").await["title"], title);
    assert_eq!(
        scratchpad(&server, &person, "TK-1").await[0]["content"],
        content
    );
    server.stop().await;
}

#[tokio::test]
async fn create_task_fails_when_person_is_viewer() {
    let (server, person, _) = setup().await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    let task = create(&server, &agent, SPACE, "Bob task").await;
    let original = read(&server, &person, "TK-1").await;
    space_member(&server, &person, Method::PATCH, "viewer").await;
    for response in [
        request(
            &server,
            &agent,
            Method::POST,
            TASKS,
            json!({"title":"Forbidden"}),
        )
        .await,
        checkpoint_request(
            &server,
            &agent,
            SPACE,
            task["task_id"].as_str().unwrap(),
            "Forbidden",
        )
        .await,
    ] {
        assert_problem(response, StatusCode::FORBIDDEN, "space.permission_required").await;
    }
    assert_eq!(read(&server, &bob, "TK-1").await, original);
    assert_eq!(scratchpad(&server, &bob, "TK-1").await, json!([]));
    server.stop().await;
}

#[tokio::test]
async fn add_checkpoint_appends_to_scratchpad_and_updates_last_activity() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Research").await;
    let initial = read(&server, &person, "TK-1").await;
    let checkpoint = append(&server, &agent, &task, "Found #KN-999\nDetails").await;
    assert_eq!(checkpoint["task_title"], "Research");
    assert!(checkpoint["time"].as_i64().unwrap() >= initial["started_at"].as_i64().unwrap());
    let mut expected = initial;
    expected["last_activity_at"] = checkpoint["time"].clone();
    assert_eq!(read(&server, &person, "TK-1").await, expected);
    assert_eq!(
        scratchpad(&server, &person, "TK-1").await,
        json!([{"time":checkpoint["time"], "content":"Found #KN-999\nDetails"}])
    );
    server.stop().await;
}

#[tokio::test]
async fn add_checkpoint_gives_each_checkpoint_a_distinct_increasing_time() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Concurrent checkpoints").await;
    let mut appends = JoinSet::new();
    for number in 0..50 {
        let client = server.client.clone();
        let url = format!(
            "http://{}{TASKS}/{}/checkpoints",
            server.address,
            task["task_id"].as_str().unwrap()
        );
        let token = agent["access_token"].as_str().unwrap().to_owned();
        appends.spawn(async move {
            let content = format!("Checkpoint {number}");
            let response = client
                .post(url)
                .bearer_auth(token)
                .json(&json!({"content":content}))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let checkpoint = response.json::<Value>().await.unwrap()["data"].clone();
            assert_eq!(checkpoint["task_title"], "Concurrent checkpoints");
            json!({"time":checkpoint["time"], "content":content})
        });
    }
    let mut expected = Vec::new();
    while let Some(result) = appends.join_next().await {
        expected.push(result.unwrap());
    }
    expected.sort_by_key(|checkpoint| checkpoint["time"].as_i64().unwrap());
    assert_eq!(expected.len(), 50);
    assert!(
        expected
            .windows(2)
            .all(|pair| pair[0]["time"].as_i64() < pair[1]["time"].as_i64())
    );
    assert_eq!(scratchpad(&server, &person, "TK-1").await, json!(expected));
    let first_page = list_page(&server, &person, &format!("{TASKS}/TK-1/scratchpad"), &[]).await;
    assert_eq!(first_page["data"], json!(&expected[..20]));
    assert!(first_page["meta"]["next_cursor"].is_string());
    assert_eq!(
        read(&server, &person, "TK-1").await["last_activity_at"],
        expected.last().unwrap()["time"]
    );
    server.stop().await;
}

#[tokio::test]
async fn add_checkpoint_fails_when_task_id_does_not_exist() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Original").await;
    create_space(&server, &person, "Other", "Other work").await;
    let id = task["task_id"].as_str().unwrap();
    let mut expected = None;
    for (space, id) in [
        (SPACE, "unknown"),
        (SPACE, "TK-1"),
        ("SPACE-2", id),
        (SPACE, &id.to_uppercase()),
    ] {
        let error = assert_problem(
            checkpoint_request(&server, &agent, space, id, "Forbidden").await,
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
    assert_eq!(scratchpad(&server, &person, "TK-1").await, json!([]));
    server.stop().await;
}

#[tokio::test]
async fn add_checkpoint_fails_when_task_belongs_to_another_party() {
    let (server, person, agent) = setup().await;
    let bob = login(&server, "bob").await;
    let bob_agent = authorize_for(&server, &bob, "Bob agent").await;
    let task = create(&server, &agent, SPACE, "Alice task").await;
    let id = task["task_id"].as_str().unwrap();
    space_member(&server, &person, Method::POST, "contributor").await;
    for role in ["contributor", "admin"] {
        space_member(&server, &person, Method::PATCH, role).await;
        assert_problem(
            checkpoint_request(&server, &bob_agent, SPACE, id, "Forbidden").await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
    }
    space_member(&server, &person, Method::DELETE, "").await;
    create_space(&server, &bob, "Bob only", "Bob work").await;
    assert_problem(
        checkpoint_request(&server, &bob_agent, "SPACE-2", id, "Forbidden").await,
        StatusCode::NOT_FOUND,
        "task.not_found",
    )
    .await;
    let missing = assert_problem(
        checkpoint_request(&server, &bob_agent, SPACE, "unknown", "Forbidden").await,
        StatusCode::NOT_FOUND,
        "space.not_found",
    )
    .await;
    assert_eq!(
        assert_problem(
            checkpoint_request(&server, &bob_agent, SPACE, id, "Forbidden").await,
            StatusCode::NOT_FOUND,
            "space.not_found"
        )
        .await,
        missing
    );
    assert_eq!(scratchpad(&server, &person, "TK-1").await, json!([]));
    server.stop().await;
}

#[tokio::test]
async fn add_checkpoint_succeeds_when_task_was_created_by_another_agent_of_same_person() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Shared party").await;
    let second = authorize_for(&server, &person, "Other agent").await;
    let checkpoint = append(&server, &second, &task, "Continuing").await;
    assert_eq!(checkpoint["task_title"], "Shared party");
    assert_eq!(
        read(&server, &person, "TK-1").await["agent_name"],
        "Research agent"
    );
    assert_eq!(
        scratchpad(&server, &second, "TK-1").await,
        json!([{"time":checkpoint["time"],"content":"Continuing"}])
    );
    server.stop().await;
}

#[tokio::test]
async fn add_checkpoint_fails_when_person_was_removed_from_space() {
    let (server, person, _) = setup().await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    let task = create(&server, &agent, SPACE, "Bob task").await;
    space_member(&server, &person, Method::DELETE, "").await;
    let missing = assert_problem(
        checkpoint_request(&server, &agent, SPACE, "unknown", "Forbidden").await,
        StatusCode::NOT_FOUND,
        "space.not_found",
    )
    .await;
    assert_eq!(
        assert_problem(
            checkpoint_request(
                &server,
                &agent,
                SPACE,
                task["task_id"].as_str().unwrap(),
                "Forbidden"
            )
            .await,
            StatusCode::NOT_FOUND,
            "space.not_found"
        )
        .await,
        missing
    );
    assert_eq!(read(&server, &person, "TK-1").await["person"], "bob");
    assert_eq!(scratchpad(&server, &person, "TK-1").await, json!([]));
    server.stop().await;
}

#[tokio::test]
async fn list_tasks_returns_newest_activity_first() {
    let (server, person, agent) = setup().await;
    let bob = login(&server, "bob").await;
    let other = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "viewer").await;
    assert_eq!(
        list_page(&server, &bob, TASKS, &[]).await["data"],
        json!([])
    );
    let mut tasks = Vec::new();
    for number in 0..5 {
        tasks.push(create(&server, &agent, SPACE, &format!("Task {number}")).await);
    }
    append(&server, &agent, &tasks[0], "Newest activity").await;
    append(&server, &agent, &tasks[0], "Still newest").await;
    let expected = walk(&server, &person, TASKS, "2", 5).await;
    assert_eq!(expected.len(), 5);
    assert_eq!(expected[0]["key"], "TK-1");
    assert!(
        expected.windows(2).all(
            |pair| pair[0]["last_activity_at"].as_i64() >= pair[1]["last_activity_at"].as_i64()
        )
    );
    for tokens in [&bob, &other] {
        assert_eq!(walk(&server, tokens, TASKS, "2", 5).await, expected);
    }
    assert_task_cursors(&server, &person, &bob).await;
    let ids: HashSet<_> = expected
        .iter()
        .map(|task| task["task_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 5);
    for task in &expected {
        assert_eq!(task["person"], "alice");
        assert_eq!(task["agent_name"], "Research agent");
        assert_eq!(task["status"], "in progress");
        let checkpoint = append(&server, &agent, task, "Found by ID").await;
        assert_eq!(checkpoint["task_title"], task["title"]);
    }
    server.stop().await;
}

#[tokio::test]
async fn read_task_returns_scratchpad_page_when_requested() {
    let (server, person, agent) = setup().await;
    let bob = login(&server, "bob").await;
    let other = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "viewer").await;
    let task = create(&server, &agent, SPACE, "Paged Scratchpad").await;
    let mut expected = Vec::new();
    for number in 0..5 {
        let content = format!("Checkpoint {number}");
        let checkpoint = append(&server, &agent, &task, &content).await;
        expected.push(json!({"time":checkpoint["time"], "content":content}));
    }
    for tokens in [&bob, &other] {
        read_checkpoint_pages(&server, tokens, "TK-1", &expected).await;
        assert_eq!(
            walk(&server, tokens, &format!("{TASKS}/TK-1/scratchpad"), "2", 5).await,
            expected
        );
        for query in [[("cursor", "invalid")], [("page_size", "2")]] {
            assert_problem(
                list_response(&format!("{TASKS}/TK-1"), &server, tokens, &query).await,
                StatusCode::BAD_REQUEST,
                "request.invalid_query",
            )
            .await;
        }
    }
    assert!(
        expected
            .windows(2)
            .all(|pair| pair[0]["time"].as_i64() < pair[1]["time"].as_i64())
    );
    assert_checkpoint_cursors(&server, &person, &bob, &expected).await;
    server.stop().await;
}

#[tokio::test]
async fn read_task_fails_when_task_is_in_another_space() {
    let (server, person, agent) = setup().await;
    create(&server, &agent, SPACE, "Original").await;
    create_space(&server, &person, "Other", "Other work").await;
    let mut expected = None;
    for path in [
        "/api/v1/spaces/SPACE-2/tasks/TK-1".to_owned(),
        format!("{TASKS}/TK-999"),
        format!("{TASKS}/TK-01"),
    ] {
        for suffix in ["", "/scratchpad"] {
            let error = assert_problem(
                request(
                    &server,
                    &person,
                    Method::GET,
                    &format!("{path}{suffix}"),
                    Value::Null,
                )
                .await,
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
    server.stop().await;
}
