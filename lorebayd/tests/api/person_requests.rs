use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::helpers::{
    Server, TEST_PASSWORD, assert_problem, authorize_for, create_space, list_page, list_response,
    login, request,
};
use crate::tasks::helpers::{create as create_task, space_member};

use helpers::*;

pub(crate) mod helpers;

#[tokio::test]
async fn submit_person_request_returns_pending_person_request_with_key() {
    let (server, owner) = setup().await;
    let person_request = submit(&server, &owner, creation("Guide")).await;
    assert_eq!(person_request["key"], "RQ-1");
    assert_eq!(person_request["action"], "create");
    assert_eq!(person_request["status"], "pending");
    assert_eq!(person_request["requested_by"], "alice");
    assert_eq!(person_request["new_name"], "Guide");
    assert_eq!(person_request["new_summary"], "Summary");
    assert_eq!(person_request["new_content"], "Content");
    assert_eq!(person_request["from_task_key"], Value::Null);
    assert_eq!(person_request["changed_since_requested"], false);
    assert_eq!(read(&server, &owner, "RQ-1").await, person_request);
    assert_eq!(
        submit(&server, &owner, creation("Next")).await["key"],
        "RQ-2"
    );
    for target in [
        json!({"action":"delete","target_name":"Missing"}),
        json!({"action":"delete","target_key":"KN-99"}),
    ] {
        assert_problem(
            request(&server, &owner, Method::POST, PATH, target).await,
            StatusCode::NOT_FOUND,
            "knowledge.not_found",
        )
        .await;
    }
    assert_eq!(
        list_page(&server, &owner, KNOWLEDGE, &[]).await["data"],
        json!([])
    );
}

#[tokio::test]
async fn submit_person_request_records_source_task_when_agent_submits() {
    let (server, owner) = setup().await;
    let agent = authorize_for(&server, &owner, "Writer").await;
    let task = create_task(&server, &agent, "SPACE-1", "Improve guide").await;
    let result = submit_from_task(&server, &agent, &task).await;
    assert_eq!(result["task_title"], "Improve guide");
    assert_eq!(result["from_task_key"], task["key"]);
    assert_eq!(result["requester_agent_name"], "Writer");
    assert_eq!(result["requested_by"], "alice");
    let mut expected = result;
    expected.as_object_mut().unwrap().remove("task_title");
    assert_eq!(read(&server, &owner, "RQ-1").await, expected);
    for (caller, path, code) in [
        (&agent, PATH.to_owned(), "auth.person_required"),
        (&owner, task_path(&task), "auth.agent_required"),
    ] {
        assert_problem(
            request(&server, caller, Method::POST, &path, creation("Denied")).await,
            StatusCode::FORBIDDEN,
            code,
        )
        .await;
    }
    create_space(&server, &owner, "Other", "Other work").await;
    let other_path = task_path(&task).replace("SPACE-1", "SPACE-2");
    assert_problem(
        request(
            &server,
            &agent,
            Method::POST,
            &other_path,
            creation("Wrong space"),
        )
        .await,
        StatusCode::NOT_FOUND,
        "task.not_found",
    )
    .await;
    submit_deleted_targets(&server, &owner, &agent, &task).await;
}

#[tokio::test]
async fn submit_person_request_succeeds_when_task_is_ended() {
    let (server, owner) = setup().await;
    let agent = authorize_for(&server, &owner, "Writer").await;
    let task = create_task(&server, &agent, "SPACE-1", "Finished work").await;
    data(
        request(
            &server,
            &agent,
            Method::PATCH,
            &format!(
                "/api/v1/spaces/SPACE-1/tasks/{}",
                task["task_id"].as_str().unwrap()
            ),
            json!({"state":"done"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        submit_from_task(&server, &agent, &task).await["status"],
        "pending"
    );
    assert_eq!(read(&server, &owner, "RQ-1").await["from_task_key"], "TK-1");
}

#[tokio::test]
async fn submit_person_request_fails_when_person_is_viewer() {
    let (server, owner) = setup().await;
    space_member(&server, &owner, Method::POST, "contributor").await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &bob, "Viewer").await;
    let task = create_task(&server, &agent, "SPACE-1", "Existing task").await;
    space_member(&server, &owner, Method::PATCH, "viewer").await;
    for (caller, path) in [(&bob, PATH.to_owned()), (&agent, task_path(&task))] {
        assert_problem(
            request(&server, caller, Method::POST, &path, creation("Denied")).await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
    }
    assert_eq!(list_page(&server, &bob, PATH, &[]).await["data"], json!([]));
    submit(&server, &owner, creation("Visible")).await;
    assert_eq!(
        read(&server, &agent, "RQ-1").await["new_content"],
        "Content"
    );
}

#[tokio::test]
async fn submit_person_request_fails_when_change_carries_no_field() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    for body in [
        json!({"action":"change","target_key":"KN-1"}),
        json!({"action":"change","target_key":"KN-1","content":null}),
        json!({"action":"change","target_name":"Guide","target_key":"KN-1","content":"New"}),
        json!({"action":"change","target_name":null,"target_key":null,"name":"Other"}),
        json!({"action":"delete","target_name":"Guide","content":"Forbidden"}),
        json!({"action":"create","name":"a/../b","summary":"Summary","content":"Body"}),
        json!({"action":"create","name":"Guide","summary":"s".repeat(501),"content":"Body"}),
        json!({"action":"create","name":"Guide","summary":"Summary","content":"a".repeat(1024*1024+1)}),
    ] {
        assert_problem(
            request(&server, &owner, Method::POST, PATH, body).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    assert_eq!(
        list_page(&server, &owner, PATH, &[]).await["data"],
        json!([])
    );
}

#[tokio::test]
async fn submit_person_request_fails_when_task_belongs_to_another_party() {
    let (server, owner) = setup().await;
    let owner_agent = authorize_for(&server, &owner, "Owner writer").await;
    let task = create_task(&server, &owner_agent, "SPACE-1", "Owner work").await;
    space_member(&server, &owner, Method::POST, "contributor").await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &bob, "Other writer").await;
    assert_problem(
        request(
            &server,
            &agent,
            Method::POST,
            &task_path(&task),
            creation("Denied"),
        )
        .await,
        StatusCode::FORBIDDEN,
        "space.permission_required",
    )
    .await;
    space_member(&server, &owner, Method::DELETE, "contributor").await;
    assert_problem(
        request(
            &server,
            &agent,
            Method::POST,
            &task_path(&task),
            creation("Denied"),
        )
        .await,
        StatusCode::NOT_FOUND,
        "space.not_found",
    )
    .await;
    assert_problem(
        request(&server, &bob, Method::POST, PATH, creation("Denied")).await,
        StatusCode::NOT_FOUND,
        "space.not_found",
    )
    .await;
    assert_problem(
        request(&server, &bob, Method::GET, &by_key("RQ-1"), Value::Null).await,
        StatusCode::NOT_FOUND,
        "space.not_found",
    )
    .await;
}

#[tokio::test]
async fn list_person_requests_returns_only_matching_status() {
    let (server, owner) = setup().await;
    for name in ["First", "Second", "Third", "Fourth"] {
        submit(&server, &owner, creation(name)).await;
    }
    approve(&server, &owner, "RQ-1").await;
    reject(&server, &owner, "RQ-2").await;
    let page = list_page(
        &server,
        &owner,
        PATH,
        &[("status", "pending"), ("page_size", "1")],
    )
    .await;
    assert_eq!(page["data"][0]["key"], "RQ-4");
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    assert_eq!(
        list_page(
            &server,
            &owner,
            PATH,
            &[("status", "pending"), ("cursor", cursor)]
        )
        .await["data"][0]["key"],
        "RQ-3"
    );
    for query in [
        vec![("status", "approved"), ("cursor", cursor)],
        vec![
            ("status", "pending"),
            ("keyword", "Third"),
            ("cursor", cursor),
        ],
        vec![("status", "unknown")],
        vec![("page_size", "0")],
        vec![("cursor", "invalid")],
    ] {
        assert_problem(
            list_response(PATH, &server, &owner, &query).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
    for (status, key) in [("approved", "RQ-1"), ("rejected", "RQ-2")] {
        let page = list_page(&server, &owner, PATH, &[("status", status)]).await;
        assert_eq!(page["data"].as_array().unwrap().len(), 1);
        assert_eq!(page["data"][0]["key"], key);
    }
    let all = list_page(&server, &owner, PATH, &[]).await;
    assert_eq!(all["data"][0]["key"], "RQ-4");
    assert_eq!(all["data"][3]["key"], "RQ-1");
}

#[tokio::test]
async fn list_person_requests_shows_target_and_proposed_summary() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    submit(&server,&owner,json!({"action":"change","target_name":"guide","name":"Renamed","summary":"Proposed summary"})).await;
    let page = list_page(&server, &owner, PATH, &[]).await;
    let row = &page["data"][0];
    assert_eq!(row["target"], json!({"key":"KN-1","name":"Guide"}));
    assert_eq!(row["new_name"], "Renamed");
    assert_eq!(row["new_summary"], "Proposed summary");
    assert_eq!(row["requested_by"], "alice");
    assert!(row.get("new_content").is_none());
    assert_eq!(read(&server, &owner, "RQ-1").await["base_version"], 1);
}

#[tokio::test]
async fn list_person_requests_matches_keyword_in_proposed_name_summary_and_target() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Target needle").await;
    submit(&server, &owner, creation("Name needle")).await;
    let mut body = creation("Other");
    body["summary"] = json!("needle A%_B\\");
    submit(&server, &owner, body).await;
    submit(
        &server,
        &owner,
        json!({"action":"change","target_key":"KN-1","content":"Unrelated"}),
    )
    .await;
    submit(&server, &owner, creation("Unrelated")).await;
    let mut cursor = None;
    let mut keys = Vec::new();
    loop {
        let mut query = vec![
            ("keyword", "NEEDLE"),
            ("status", "pending"),
            ("page_size", "1"),
        ];
        if let Some(cursor) = cursor.as_deref() {
            query.push(("cursor", cursor));
        }
        let page = list_page(&server, &owner, PATH, &query).await;
        keys.push(page["data"][0]["key"].as_str().unwrap().to_owned());
        let Some(next) = page["meta"]["next_cursor"].as_str() else {
            break;
        };
        cursor = Some(next.to_owned());
        assert!(keys.len() < 4);
    }
    assert_eq!(keys, ["RQ-3", "RQ-2", "RQ-1"]);
    for keyword in ["%", "_", "\\", "a%_b\\"] {
        let page = list_page(&server, &owner, PATH, &[("keyword", keyword)]).await;
        assert_eq!(page["data"].as_array().unwrap().len(), 1);
        assert_eq!(page["data"][0]["key"], "RQ-2");
    }
    reject(&server, &owner, "RQ-2").await;
    assert_eq!(
        list_page(
            &server,
            &owner,
            PATH,
            &[("status", "pending"), ("keyword", "a%_b\\")]
        )
        .await["data"],
        json!([])
    );
    for keyword in [String::new(), " ".to_owned(), "a".repeat(201)] {
        assert_problem(
            list_response(PATH, &server, &owner, &[("keyword", &keyword)]).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
}

#[tokio::test]
async fn submit_person_request_accepts_content_at_size_limit() {
    let (server, owner) = setup().await;
    let agent = authorize_for(&server, &owner, "Writer").await;
    let task = create_task(&server, &agent, "SPACE-1", "Large request").await;
    seed(&server, &owner, &"\u{1f680}".repeat(200)).await;
    for (caller, path) in [(&owner, PATH.to_owned()), (&agent, task_path(&task))] {
        for action in ["create", "change"] {
            let target = if action == "change" {
                format!(",\"target_name\":\"{}\"", "\\ud83d\\ude80".repeat(200))
            } else {
                String::new()
            };
            let body = format!(
                "{{\"action\":\"{action}\",\"name\":\"{}\",\"summary\":\"{}\",\"content\":\"{}\"{target}}}",
                "\\ud83d\\ude80".repeat(200),
                "\\ud83d\\ude80".repeat(500),
                "\\u0061".repeat(1024 * 1024)
            );
            let padded = format!("{body}{}", " ".repeat(BODY_LIMIT - body.len()));
            let result = data(
                raw(&server, caller, &path, padded).await,
                StatusCode::CREATED,
            )
            .await;
            assert_eq!(result["new_content"], "a".repeat(1024 * 1024));
            assert_eq!(
                read(&server, &owner, result["key"].as_str().unwrap()).await["new_summary"],
                "\u{1f680}".repeat(500)
            );
            assert_problem(
                raw(
                    &server,
                    caller,
                    &path,
                    format!("{body}{}", " ".repeat(BODY_LIMIT + 1 - body.len())),
                )
                .await,
                StatusCode::PAYLOAD_TOO_LARGE,
                "request.body_too_large",
            )
            .await;
        }
    }
}

#[tokio::test]
async fn read_person_request_marks_changed_since_requested_when_entry_has_newer_version() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    submit(
        &server,
        &owner,
        json!({"action":"change","target_key":"KN-1","content":"Proposed"}),
    )
    .await;
    data(
        request(
            &server,
            &owner,
            Method::PATCH,
            &format!("{KNOWLEDGE}/by-key/KN-1"),
            json!({"name":"Renamed"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        read(&server, &owner, "RQ-1").await["changed_since_requested"],
        false
    );
    data(
        request(
            &server,
            &owner,
            Method::PATCH,
            &format!("{KNOWLEDGE}/by-key/KN-1"),
            json!({"summary":"Changed"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        read(&server, &owner, "RQ-1").await["changed_since_requested"],
        true
    );
    reject(&server, &owner, "RQ-1").await;
    assert!(
        read(&server, &owner, "RQ-1")
            .await
            .get("changed_since_requested")
            .is_none()
    );
}

#[tokio::test]
async fn read_person_request_marks_changed_since_requested_when_create_name_exists() {
    let (server, owner) = setup().await;
    submit(&server, &owner, creation("Guide")).await;
    assert_eq!(
        read(&server, &owner, "RQ-1").await["changed_since_requested"],
        false
    );
    seed(&server, &owner, "GUIDE").await;
    assert_eq!(
        read(&server, &owner, "RQ-1").await["changed_since_requested"],
        true
    );
    data(
        request(
            &server,
            &owner,
            Method::DELETE,
            &format!("{KNOWLEDGE}/by-key/KN-1"),
            Value::Null,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(
        read(&server, &owner, "RQ-1").await["changed_since_requested"],
        true
    );
    approve(&server, &owner, "RQ-1").await;
    assert!(
        read(&server, &owner, "RQ-1")
            .await
            .get("changed_since_requested")
            .is_none()
    );
    let restored = entry(&server, &owner, "KN-1").await;
    assert_eq!(restored["name"], "GUIDE");
    assert_eq!(restored["content"], "Content");
}

#[tokio::test]
async fn read_person_request_keeps_person_request_when_source_task_was_deleted() {
    let (server, owner) = setup().await;
    let agent = authorize_for(&server, &owner, "Writer").await;
    let task = create_task(&server, &agent, "SPACE-1", "Work").await;
    submit_from_task(&server, &agent, &task).await;
    let response = request(
        &server,
        &owner,
        Method::DELETE,
        &format!(
            "/api/v1/spaces/SPACE-1/tasks/{}",
            task["task_id"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let person_request = read(&server, &owner, "RQ-1").await;
    assert_eq!(person_request["from_task_key"], Value::Null);
    assert_eq!(person_request["new_content"], "Content");
    assert_eq!(person_request["requester_agent_name"], "Writer");
}

#[tokio::test]
async fn approve_person_request_creates_entry_that_agents_can_read() {
    let (server, owner) = setup().await;
    space_member(&server, &owner, Method::POST, "contributor").await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &bob, "Reader").await;
    let proposed = submit(&server, &bob, creation("Guide")).await;
    let approved = approve(&server, &owner, proposed["key"].as_str().unwrap()).await;
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["decided_by"], "alice");
    assert!(approved["decided_at"].as_i64().unwrap() > 0);
    assert_eq!(approved["target"], json!({"key":"KN-1","name":"Guide"}));
    assert_eq!(entry(&server, &agent, "KN-1").await["content"], "Content");
    assert_eq!(read(&server, &bob, "RQ-1").await, approved);
}

#[tokio::test]
async fn approve_person_request_fails_when_contributor_approves_own_partys_person_request() {
    let (server, owner) = setup().await;
    space_member(&server, &owner, Method::POST, "contributor").await;
    let bob = login(&server, "bob").await;
    submit(&server, &bob, creation("Person guide")).await;
    let agent = authorize_for(&server, &bob, "Writer").await;
    let task = create_task(&server, &agent, "SPACE-1", "Work").await;
    submit_from_task(&server, &agent, &task).await;
    for key in ["RQ-1", "RQ-2"] {
        assert_problem(
            request(
                &server,
                &bob,
                Method::POST,
                &format!("{}/approve", by_key(key)),
                Value::Null,
            )
            .await,
            StatusCode::FORBIDDEN,
            "person_request.own_party",
        )
        .await;
        assert_eq!(read(&server, &bob, key).await["status"], "pending");
    }
}

#[tokio::test]
async fn approve_person_request_succeeds_when_admin_approves_own_person_request() {
    let (server, owner) = setup().await;
    space_member(&server, &owner, Method::POST, "contributor").await;
    space_member(&server, &owner, Method::PATCH, "admin").await;
    let bob = login(&server, "bob").await;
    submit(&server, &bob, creation("Admin guide")).await;
    assert_eq!(approve(&server, &bob, "RQ-1").await["decided_by"], "bob");
    assert_eq!(entry(&server, &bob, "KN-1").await["applied_by"], "bob");
}

#[tokio::test]
async fn approve_person_request_succeeds_when_another_contributor_approves() {
    let server = Server::restart_with_users(
        TempDir::new().unwrap(),
        &[
            ("alice", TEST_PASSWORD),
            ("bob", TEST_PASSWORD),
            ("charlie", TEST_PASSWORD),
        ],
    )
    .await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared").await;
    for person in ["bob", "charlie"] {
        data(
            request(
                &server,
                &owner,
                Method::POST,
                &format!("/api/v1/spaces/SPACE-1/members/{person}"),
                json!({"role":"contributor"}),
            )
            .await,
            StatusCode::CREATED,
        )
        .await;
    }
    let bob = login(&server, "bob").await;
    let charlie = login(&server, "charlie").await;
    submit(&server, &bob, creation("Guide")).await;
    assert_eq!(
        approve(&server, &charlie, "RQ-1").await["decided_by"],
        "charlie"
    );
    assert_eq!(entry(&server, &bob, "KN-1").await["applied_by"], "charlie");
}

#[tokio::test]
async fn approve_person_request_fails_when_caller_is_agent() {
    let (server, owner) = setup().await;
    submit(&server, &owner, creation("Guide")).await;
    let agent = authorize_for(&server, &owner, "Writer").await;
    for (action, body) in [
        ("approve", Value::Null),
        ("reject", json!({"reason":"Denied"})),
    ] {
        assert_problem(
            request(
                &server,
                &agent,
                Method::POST,
                &format!("{}/{action}", by_key("RQ-1")),
                body,
            )
            .await,
            StatusCode::FORBIDDEN,
            "auth.person_required",
        )
        .await;
    }
    assert_eq!(read(&server, &owner, "RQ-1").await["status"], "pending");
}

#[tokio::test]
async fn approve_person_request_fails_when_person_is_viewer() {
    let (server, owner) = setup().await;
    submit(&server, &owner, creation("Guide")).await;
    space_member(&server, &owner, Method::POST, "viewer").await;
    let bob = login(&server, "bob").await;
    for (action, body) in [
        ("approve", Value::Null),
        ("reject", json!({"reason":"Denied"})),
    ] {
        assert_problem(
            request(
                &server,
                &bob,
                Method::POST,
                &format!("{}/{action}", by_key("RQ-1")),
                body,
            )
            .await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
    }
    assert_eq!(read(&server, &bob, "RQ-1").await["status"], "pending");
}

#[tokio::test]
async fn approve_person_request_fails_when_rename_target_belongs_to_another_entry() {
    let (server, owner) = setup().await;
    let original = seed(&server, &owner, "Guide").await;
    seed(&server, &owner, "Other").await;
    submit(
        &server,
        &owner,
        json!({"action":"change","target_name":"guide","name":"OTHER","content":"Must not apply"}),
    )
    .await;
    for deleted in [false, true] {
        if deleted {
            data(
                request(
                    &server,
                    &owner,
                    Method::DELETE,
                    &format!("{KNOWLEDGE}/by-key/KN-2"),
                    Value::Null,
                )
                .await,
                StatusCode::OK,
            )
            .await;
        }
        let error = assert_problem(
            request(
                &server,
                &owner,
                Method::POST,
                &format!("{}/approve", by_key("RQ-1")),
                Value::Null,
            )
            .await,
            StatusCode::CONFLICT,
            "knowledge.name_taken",
        )
        .await;
        assert!(!error["title"].as_str().unwrap().contains("roll it back"));
        assert_eq!(read(&server, &owner, "RQ-1").await["status"], "pending");
        assert_eq!(entry(&server, &owner, "KN-1").await, original);
    }
}

#[tokio::test]
async fn approve_person_request_fails_when_person_request_is_already_decided() {
    let (server, owner) = setup().await;
    submit(&server, &owner, creation("First")).await;
    submit(&server, &owner, creation("Second")).await;
    approve(&server, &owner, "RQ-1").await;
    reject(&server, &owner, "RQ-2").await;
    for key in ["RQ-1", "RQ-2", "RQ-99", "invalid"] {
        for (action, body) in [
            ("approve", Value::Null),
            ("reject", json!({"reason":"Again"})),
        ] {
            let (status, code) = if key == "RQ-1" || key == "RQ-2" {
                (StatusCode::CONFLICT, "person_request.decided")
            } else {
                (StatusCode::NOT_FOUND, "person_request.not_found")
            };
            assert_problem(
                request(
                    &server,
                    &owner,
                    Method::POST,
                    &format!("{}/{action}", by_key(key)),
                    body,
                )
                .await,
                status,
                code,
            )
            .await;
        }
    }
    assert_problem(
        request(&server, &owner, Method::GET, &by_key("RQ-99"), Value::Null).await,
        StatusCode::NOT_FOUND,
        "person_request.not_found",
    )
    .await;
}

#[tokio::test]
async fn reject_person_request_keeps_person_request_with_reason() {
    let (server, owner) = setup().await;
    submit(&server, &owner, creation("Guide")).await;
    let rejected = reject(&server, &owner, "RQ-1").await;
    assert_eq!(rejected["status"], "rejected");
    assert_eq!(rejected["new_content"], "Content");
    assert_eq!(rejected["reject_reason"], "Superseded");
    assert_eq!(rejected["requested_by"], "alice");
    assert_eq!(rejected["decided_by"], "alice");
    assert!(rejected["decided_at"].as_i64().unwrap() > 0);
    space_member(&server, &owner, Method::POST, "viewer").await;
    let bob = login(&server, "bob").await;
    assert_eq!(read(&server, &bob, "RQ-1").await, rejected);
    assert_eq!(
        list_page(&server, &owner, KNOWLEDGE, &[]).await["data"],
        json!([])
    );
}

#[tokio::test]
async fn reject_person_request_fails_when_reason_is_missing() {
    let (server, owner) = setup().await;
    submit(&server, &owner, creation("Guide")).await;
    for body in [
        json!({}),
        json!({"reason":""}),
        json!({"reason":" "}),
        json!({"reason":"a".repeat(2001)}),
    ] {
        assert_problem(
            request(
                &server,
                &owner,
                Method::POST,
                &format!("{}/reject", by_key("RQ-1")),
                body,
            )
            .await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    assert_eq!(read(&server, &owner, "RQ-1").await["status"], "pending");
    let reason = "\u{1f680}".repeat(2000);
    let result = data(
        request(
            &server,
            &owner,
            Method::POST,
            &format!("{}/reject", by_key("RQ-1")),
            json!({"reason":reason}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(result["reject_reason"], reason);
}

#[tokio::test]
async fn reject_person_request_withdraws_own_partys_person_request() {
    let (server, owner) = setup().await;
    space_member(&server, &owner, Method::POST, "contributor").await;
    let bob = login(&server, "bob").await;
    submit(&server, &bob, creation("Person guide")).await;
    let agent = authorize_for(&server, &bob, "Writer").await;
    let task = create_task(&server, &agent, "SPACE-1", "Work").await;
    submit_from_task(&server, &agent, &task).await;
    for key in ["RQ-1", "RQ-2"] {
        assert_eq!(reject(&server, &bob, key).await["status"], "rejected");
        assert_eq!(read(&server, &agent, key).await["decided_by"], "bob");
    }
    assert_eq!(
        list_page(&server, &owner, KNOWLEDGE, &[]).await["data"],
        json!([])
    );
}

#[tokio::test]
async fn approve_person_request_adds_version_with_approver_and_person_request() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    let before = versions(&server, &owner, "KN-1").await;
    assert_eq!(before.as_array().unwrap().len(), 1);
    assert_version(&before[0], 1, "content", "alice", None);
    space_member(&server, &owner, Method::POST, "contributor").await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &owner, "Writer").await;
    let task = create_task(&server, &agent, "SPACE-1", "Revise guide").await;
    let person_request = data(request(&server, &agent, Method::POST, &task_path(&task),
        json!({"action":"change","target_key":"KN-1","summary":"Approved summary","content":"Approved content"})).await,
        StatusCode::CREATED).await;
    assert_eq!(person_request["requested_by"], "alice");
    assert_eq!(person_request["task_title"], "Revise guide");
    assert_eq!(approve(&server, &bob, "RQ-1").await["decided_by"], "bob");
    let latest = entry(&server, &agent, "KN-1").await;
    assert_eq!(latest["content"], "Approved content");
    assert_eq!(latest["summary"], "Approved summary");
    let history = versions(&server, &bob, "KN-1").await;
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_version(&history[0], 2, "content", "bob", Some("RQ-1"));
    assert_eq!(history[0]["applied_at"], latest["applied_at"]);
    assert_eq!(history[1], before[0]);
    assert_eq!(
        old_entry(&server, &bob, "KN-1", 1).await["content"],
        "Content"
    );
}

#[tokio::test]
async fn approve_person_request_applies_person_request_content_when_entry_changed_since_requested()
{
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    submit(
        &server,
        &owner,
        json!({"action":"change","target_name":"guide","content":"Proposed content"}),
    )
    .await;
    change_entry(
        &server,
        &owner,
        "KN-1",
        json!({"content":"Intervening content","summary":"Current summary"}),
    )
    .await;
    let before = versions(&server, &owner, "KN-1").await;
    assert_eq!(before.as_array().unwrap().len(), 2);
    assert_version(&before[0], 2, "content", "alice", None);
    assert_eq!(
        read(&server, &owner, "RQ-1").await["changed_since_requested"],
        true
    );
    approve(&server, &owner, "RQ-1").await;
    let latest = entry(&server, &owner, "KN-1").await;
    assert_eq!(latest["content"], "Proposed content");
    assert_eq!(latest["summary"], "Current summary");
    let history = versions(&server, &owner, "KN-1").await;
    assert_eq!(history.as_array().unwrap().len(), 3);
    assert_version(&history[0], 3, "content", "alice", Some("RQ-1"));
    assert_eq!(
        &history.as_array().unwrap()[1..],
        before.as_array().unwrap()
    );
    let intervening = old_entry(&server, &owner, "KN-1", 2).await;
    assert_eq!(intervening["content"], "Intervening content");
    assert_eq!(intervening["summary"], "Current summary");
    assert_eq!(
        old_entry(&server, &owner, "KN-1", 1).await["content"],
        "Content"
    );
}

#[tokio::test]
async fn approve_person_request_restores_deleted_entry_with_person_request_content() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    submit(
        &server,
        &owner,
        json!({"action":"change","target_key":"KN-1","content":"Restored content"}),
    )
    .await;
    delete_entry(&server, &owner, "KN-1").await;
    let before = versions(&server, &owner, "KN-1").await;
    assert_version(&before[0], 2, "deletion", "alice", None);
    approve(&server, &owner, "RQ-1").await;
    let agent = authorize_for(&server, &owner, "Reader").await;
    let restored = entry(&server, &agent, "KN-1").await;
    assert_eq!(restored["content"], "Restored content");
    assert_eq!(restored["summary"], "Summary");
    let history = versions(&server, &owner, "KN-1").await;
    assert_eq!(history.as_array().unwrap().len(), 3);
    assert_version(&history[0], 3, "content", "alice", Some("RQ-1"));
    assert_eq!(
        &history.as_array().unwrap()[1..],
        before.as_array().unwrap()
    );
    assert_eq!(
        old_entry(&server, &owner, "KN-1", 2).await["kind"],
        "deletion"
    );
    delete_entry(&server, &owner, "KN-1").await;
    let before_rename = versions(&server, &owner, "KN-1").await;
    let person_request = submit(
        &server,
        &owner,
        json!({"action":"change","target_name":"guide","name":"docs/Guide"}),
    )
    .await;
    assert_eq!(person_request["base_version"], 4);
    approve(&server, &owner, "RQ-2").await;
    let renamed = entry(&server, &agent, "KN-1").await;
    assert_eq!(renamed["name"], "docs/Guide");
    assert_eq!(renamed["content"], "Restored content");
    assert_eq!(renamed["summary"], "Summary");
    let history = versions(&server, &owner, "KN-1").await;
    assert_eq!(history.as_array().unwrap().len(), 5);
    assert_version(&history[0], 5, "content", "alice", Some("RQ-2"));
    assert_eq!(
        &history.as_array().unwrap()[1..],
        before_rename.as_array().unwrap()
    );
    assert_eq!(
        list_page(&server, &owner, &format!("{KNOWLEDGE}/deleted"), &[]).await["data"],
        json!([])
    );
}

#[tokio::test]
async fn approve_person_request_adds_version_when_create_name_already_exists() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    let before = versions(&server, &owner, "KN-1").await;
    submit(&server, &owner, json!({"action":"create","name":"gUIDE","summary":"Replacement summary","content":"Replacement content"})).await;
    approve(&server, &owner, "RQ-1").await;
    let replaced = entry(&server, &owner, "KN-1").await;
    assert_eq!(replaced["name"], "Guide");
    assert_eq!(replaced["content"], "Replacement content");
    assert_eq!(replaced["summary"], "Replacement summary");
    let history = versions(&server, &owner, "KN-1").await;
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_version(&history[0], 2, "content", "alice", Some("RQ-1"));
    assert_eq!(history[1], before[0]);
    delete_entry(&server, &owner, "KN-1").await;
    let before_restore = versions(&server, &owner, "KN-1").await;
    submit(&server, &owner, json!({"action":"create","name":"GUIDE","summary":"Restored summary","content":"Restored content"})).await;
    let approved = approve(&server, &owner, "RQ-2").await;
    assert_eq!(approved["target"], json!({"key":"KN-1","name":"Guide"}));
    let restored = entry(&server, &owner, "KN-1").await;
    assert_eq!(restored["name"], "Guide");
    assert_eq!(restored["content"], "Restored content");
    assert_eq!(restored["summary"], "Restored summary");
    let history = versions(&server, &owner, "KN-1").await;
    assert_eq!(history.as_array().unwrap().len(), 4);
    assert_version(&history[0], 4, "content", "alice", Some("RQ-2"));
    assert_eq!(
        &history.as_array().unwrap()[1..],
        before_restore.as_array().unwrap()
    );
    let listed = list_page(&server, &owner, KNOWLEDGE, &[]).await;
    assert_eq!(listed["data"].as_array().unwrap().len(), 1);
    assert_eq!(listed["data"][0]["key"], "KN-1");
}

#[tokio::test]
async fn approve_person_request_renames_entry_without_version() {
    let (server, owner) = setup().await;
    let original = seed(&server, &owner, "Guide").await;
    let before = versions(&server, &owner, "KN-1").await;
    submit(
        &server,
        &owner,
        json!({"action":"change","target_name":"Guide","name":"docs/Guide"}),
    )
    .await;
    approve(&server, &owner, "RQ-1").await;
    let renamed = entry(&server, &owner, "KN-1").await;
    assert_eq!(renamed["name"], "docs/Guide");
    for field in [
        "key",
        "version",
        "content",
        "summary",
        "applied_by",
        "applied_at",
    ] {
        assert_eq!(renamed[field], original[field]);
    }
    assert_eq!(versions(&server, &owner, "KN-1").await, before);
    let by_name = list_page(
        &server,
        &owner,
        &format!("{KNOWLEDGE}/entry/versions"),
        &[("name", "docs/Guide")],
    )
    .await;
    assert_eq!(by_name["data"], before);
    assert_version(&by_name["data"][0], 1, "content", "alice", None);
}

#[tokio::test]
async fn approve_person_request_deletes_entry_when_action_is_delete() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    let before = versions(&server, &owner, "KN-1").await;
    submit(
        &server,
        &owner,
        json!({"action":"delete","target_key":"KN-1"}),
    )
    .await;
    approve(&server, &owner, "RQ-1").await;
    let deleted = entry(&server, &owner, "KN-1").await;
    assert_eq!(deleted["kind"], "deletion");
    assert_eq!(deleted["content"], Value::Null);
    assert_eq!(deleted["summary"], Value::Null);
    let history = versions(&server, &owner, "KN-1").await;
    assert_eq!(history.as_array().unwrap().len(), 2);
    assert_version(&history[0], 2, "deletion", "alice", Some("RQ-1"));
    assert_eq!(history[1], before[0]);
    assert_eq!(
        old_entry(&server, &owner, "KN-1", 1).await["content"],
        "Content"
    );
    let agent = authorize_for(&server, &owner, "Reader").await;
    assert_problem(
        request(
            &server,
            &agent,
            Method::GET,
            &format!("{KNOWLEDGE}/by-key/KN-1"),
            Value::Null,
        )
        .await,
        StatusCode::NOT_FOUND,
        "knowledge.not_found",
    )
    .await;
    let second = submit(
        &server,
        &owner,
        json!({"action":"delete","target_name":"guide"}),
    )
    .await;
    assert_eq!(second["base_version"], 2);
    assert_eq!(approve(&server, &owner, "RQ-2").await["status"], "approved");
    assert_eq!(versions(&server, &owner, "KN-1").await, history);
    assert_eq!(entry(&server, &owner, "KN-1").await, deleted);
    assert_eq!(
        list_page(&server, &owner, KNOWLEDGE, &[]).await["data"],
        json!([])
    );
}
