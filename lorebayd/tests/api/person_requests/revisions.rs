use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, assert_problem, authorize_for, list_page, login, request};
use crate::mcp::tools::{call, call_error};
use crate::tasks::helpers::{create as create_task, space_member};

use super::helpers::*;

#[tokio::test]
async fn revise_request_keeps_key_and_records_person_and_each_agent() {
    let (server, person) = setup().await;
    let author = authorize_for(&server, &person, "Submitting agent").await;
    let peer = authorize_for(&server, &person, "Revising agent").await;
    let task = create_task(&server, &author, "SPACE-1", "Review guide").await;
    let first = call(&server, &author, "create_request", json!({
        "task_id":task["task_id"],"action":"create","name":"Guide","summary":"Summary","content":"Original"
    })).await;
    assert_eq!(first["revision"], 1);
    assert_eq!(first["reviser_agent_name"], Value::Null);
    let second = data(
        request(
            &server,
            &person,
            Method::PATCH,
            &by_key("RQ-1"),
            json!({"summary":"Person revision"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(second["revision"], 2);
    assert_eq!(second["revised_by"], "alice");
    assert_eq!(second["reviser_agent_name"], Value::Null);
    assert!(second["revised_at"].as_i64().unwrap() > 0);
    let third = call(
        &server,
        &peer,
        "revise_request",
        json!({
            "space":"SPACE-1","key":"RQ-1","name":"Revised guide","content":"Agent revision"
        }),
    )
    .await;
    assert_eq!(third["key"], first["key"]);
    assert_eq!(third["revision"], 3);
    assert_eq!(third["requester_agent_name"], "Submitting agent");
    assert_eq!(third["reviser_agent_name"], "Revising agent");
    assert_eq!(third["new_summary"], "Person revision");
    assert_eq!(read(&server, &person, "RQ-1").await, third);
    assert_list_attribution(&server, &peer, &third).await;
    refuse_other_decisions(&server, &person, &third).await;
    let approved = approve_revision(&server, &person, "RQ-1", 3).await;
    assert_eq!(approved["status"], "approved");
    assert_eq!(
        entry(&server, &person, "KN-1").await["content"],
        "Agent revision"
    );
    assert_eq!(
        call_error(
            &server,
            &peer,
            "revise_request",
            json!({"space":"SPACE-1","key":"RQ-1"})
        )
        .await["code"],
        "person_request.decided"
    );
    server.stop().await;
}

#[tokio::test]
async fn approve_request_refuses_changed_content_summary_and_name_until_revised() {
    let (server, person) = setup().await;
    let agent = authorize_for(&server, &person, "Reconciler").await;
    seed(&server, &person, "Guide").await;
    for (index, changes) in [
        json!({"content":"Concurrent content"}),
        json!({"summary":"Concurrent summary"}),
        json!({"name":"Renamed guide"}),
    ]
    .into_iter()
    .enumerate()
    {
        let key = format!("RQ-{}", index + 1);
        let initial = submit(
            &server,
            &person,
            json!({"action":"change","target_key":"KN-1","content":"Proposed"}),
        )
        .await;
        let current = change_entry(&server, &person, "KN-1", changes).await;
        let outdated = read(&server, &person, &key).await;
        assert_eq!(outdated["outdated"], true);
        let listed = call(
            &server,
            &agent,
            "list_requests",
            json!({"space":"SPACE-1","status":"pending"}),
        )
        .await;
        assert_eq!(listed["data"][0]["outdated"], true);
        refuse_outdated(&server, &person, &key).await;
        assert_eq!(entry(&server, &person, "KN-1").await, current);
        assert_eq!(read(&server, &person, &key).await, outdated);
        let revised = call(
            &server,
            &agent,
            "revise_request",
            json!({"space":"SPACE-1","key":key,"content":"Reconciled"}),
        )
        .await;
        assert_eq!(revised["revision"], 2);
        assert_eq!(revised["base_version"], current["version"]);
        assert_eq!(revised["base_name"], current["name"]);
        assert_eq!(revised["outdated"], false);
        assert_eq!(revised["key"], initial["key"]);
        let approved = approve_revision(&server, &person, &key, 2).await;
        assert_eq!(approved["status"], "approved");
        assert_eq!(
            entry(&server, &person, "KN-1").await["content"],
            "Reconciled"
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn approve_create_refuses_taken_name_and_revise_delete_refreshes_snapshot() {
    let (server, person) = setup().await;
    submit(&server, &person, creation("Guide")).await;
    let original = seed(&server, &person, "GUIDE").await;
    assert_problem(
        request(
            &server,
            &person,
            Method::POST,
            &format!("{}/approve", by_key("RQ-1")),
            json!({"revision":1}),
        )
        .await,
        StatusCode::CONFLICT,
        "person_request.outdated",
    )
    .await;
    assert_eq!(entry(&server, &person, "KN-1").await, original);
    data(
        request(
            &server,
            &person,
            Method::PATCH,
            &by_key("RQ-1"),
            json!({"name":"Available"}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    approve_revision(&server, &person, "RQ-1", 2).await;
    submit(
        &server,
        &person,
        json!({"action":"delete","target_key":"KN-1"}),
    )
    .await;
    change_entry(&server, &person, "KN-1", json!({"summary":"Changed"})).await;
    assert_problem(
        request(
            &server,
            &person,
            Method::POST,
            &format!("{}/approve", by_key("RQ-2")),
            json!({"revision":1}),
        )
        .await,
        StatusCode::CONFLICT,
        "person_request.outdated",
    )
    .await;
    let refreshed = revise(&server, &person, "RQ-2", json!({})).await;
    assert_eq!(refreshed["outdated"], false);
    approve_revision(&server, &person, "RQ-2", 2).await;
    assert_eq!(entry(&server, &person, "KN-1").await["kind"], "deletion");
    server.stop().await;
}

#[tokio::test]
async fn direct_knowledge_writes_refuse_old_version_and_apply_nothing() {
    let (server, person) = setup().await;
    seed(&server, &person, "Guide").await;
    let current = change_entry(&server, &person, "KN-1", json!({"content":"Current"})).await;
    let path = format!("{KNOWLEDGE}/by-key/KN-1");
    for (method, suffix, body) in [
        (
            Method::PATCH,
            "",
            json!({"version":1,"name":"Must not apply","content":"Lost"}),
        ),
        (Method::DELETE, "?version=1", Value::Null),
        (
            Method::POST,
            "/rollback",
            json!({"version":1,"replaced_version":1}),
        ),
    ] {
        assert_problem(
            request(&server, &person, method, &format!("{path}{suffix}"), body).await,
            StatusCode::CONFLICT,
            "knowledge.version_changed",
        )
        .await;
        assert_eq!(entry(&server, &person, "KN-1").await, current);
        assert_eq!(
            versions(&server, &person, "KN-1")
                .await
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    refuse_missing_versions(&server, &person, &path).await;
    let restored = data(
        request(
            &server,
            &person,
            Method::POST,
            &format!("{path}/rollback"),
            json!({"version":1,"replaced_version":2}),
        )
        .await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(restored["version"], 3);
    assert_eq!(restored["content"], "Content");
    server.stop().await;
}

async fn refuse_other_decisions(server: &Server, person: &Value, third: &Value) {
    for action in ["approve", "reject"] {
        assert_problem(
            request(
                server,
                person,
                Method::POST,
                &format!("{}/{action}", by_key("RQ-1")),
                if action == "approve" {
                    json!({"revision":2})
                } else {
                    json!({"revision":2,"reason":"Old decision"})
                },
            )
            .await,
            StatusCode::CONFLICT,
            "person_request.revision_changed",
        )
        .await;
    }
    assert_eq!(read(server, person, "RQ-1").await, *third);
    assert_eq!(
        list_page(server, person, KNOWLEDGE, &[]).await["data"],
        json!([])
    );
    space_member(server, person, Method::POST, "contributor").await;
    let bob = login(server, "bob").await;
    let other = authorize_for(server, &bob, "Other party").await;
    assert_problem(
        request(
            server,
            &bob,
            Method::PATCH,
            &by_key("RQ-1"),
            json!({"content":"Forbidden"}),
        )
        .await,
        StatusCode::FORBIDDEN,
        "space.permission_required",
    )
    .await;
    assert_eq!(
        call_error(
            server,
            &other,
            "revise_request",
            json!({"space":"SPACE-1","key":"RQ-1","content":"Forbidden"})
        )
        .await["code"],
        "space.permission_required"
    );
}

async fn refuse_missing_versions(server: &Server, person: &Value, path: &str) {
    for (method, suffix, body) in [
        (Method::PATCH, "", json!({"content":"Missing version"})),
        (Method::DELETE, "", Value::Null),
        (Method::POST, "/rollback", json!({"version":1})),
    ] {
        let empty_body = body.is_null();
        assert_problem(
            request(server, person, method, &format!("{path}{suffix}"), body).await,
            StatusCode::BAD_REQUEST,
            if suffix.is_empty() && empty_body {
                "request.invalid_query"
            } else {
                "request.invalid_body"
            },
        )
        .await;
    }
}

async fn assert_list_attribution(server: &Server, peer: &Value, third: &Value) {
    let listed = call(server, peer, "list_requests", json!({"space":"SPACE-1"})).await;
    for field in [
        "revision",
        "outdated",
        "requester_agent_name",
        "reviser_agent_name",
        "revised_by",
        "revised_at",
    ] {
        assert_eq!(listed["data"][0][field], third[field]);
    }
}

async fn refuse_outdated(server: &Server, person: &Value, key: &str) {
    assert_problem(
        request(
            server,
            person,
            Method::POST,
            &format!("{}/approve", by_key(key)),
            json!({"revision":1}),
        )
        .await,
        StatusCode::CONFLICT,
        "person_request.outdated",
    )
    .await;
}
