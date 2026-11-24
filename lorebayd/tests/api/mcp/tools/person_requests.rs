use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, authorize_for, list_page, login};
use crate::knowledge::helpers::{CONTENT_LIMIT, padded_body};
use crate::person_requests::helpers::{self as person_requests, PATH};
use crate::tasks::helpers::{self as tasks, SPACE};

use super::super::{ACCEPT_TYPES, NEWEST_VERSION, VERSION_HEADER, request_body};
use super::{call, call_error};

#[tokio::test]
async fn call_create_person_request_returns_task_title_and_pending_person_request() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Improve guide").await;
    person_requests::seed(&server, &person, "Existing").await;
    call(
        &server,
        &agent,
        "update_task",
        json!({"task_id":task["task_id"], "state":"done"}),
    )
    .await;
    for (index, body) in [
        person_requests::creation("New guide"),
        json!({"action":"change", "target_name":"Existing", "name":"Renamed", "summary":"New summary", "content":"New content"}),
        json!({"action":"delete", "target_key":"KN-1"}),
    ].into_iter().enumerate() {
        let mut arguments = body;
        arguments["task_id"] = task["task_id"].clone();
        let result = call(&server, &agent, "create_request", arguments).await;
        assert_eq!(result["task_title"], "Improve guide");
        assert_eq!(result["status"], "pending");
        assert_eq!(result["requester_agent_name"], "Research agent");
        assert_eq!(result["requested_by"], "alice");
        assert_eq!(result["from_task_key"], "TK-1");
        let key = format!("RQ-{}", index + 1);
        let mut read = person_requests::read(&server, &person, &key).await;
        read["task_title"] = json!("Improve guide");
        assert_eq!(result, read);
    }
    assert_eq!(
        person_requests::entry(&server, &person, "KN-1").await["name"],
        "Existing"
    );
    assert_eq!(tasks::read(&server, &person, "TK-1").await["state"], "done");
    assert_invalid_person_requests(&server, &agent, &task).await;
    assert_eq!(
        list_page(&server, &person, PATH, &[]).await["data"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    server.stop().await;
}

#[tokio::test]
async fn call_create_person_request_fails_when_task_belongs_to_another_party() {
    let (server, person, agent) = tasks::setup().await;
    tasks::space_member(&server, &person, Method::POST, "contributor").await;
    let bob = login(&server, "bob").await;
    let other = authorize_for(&server, &bob, "Other party").await;
    let task = tasks::create(&server, &agent, SPACE, "Private write target").await;
    let mut arguments = person_requests::creation("Forbidden");
    arguments["task_id"] = task["task_id"].clone();
    assert_eq!(
        call_error(&server, &other, "create_request", arguments).await["code"],
        "space.permission_required"
    );
    assert_eq!(
        list_page(&server, &person, PATH, &[]).await["data"],
        json!([])
    );
    server.stop().await;
}

#[tokio::test]
async fn call_create_person_request_accepts_content_at_size_limit() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Large request").await;
    let target = "\u{1f600}".repeat(200);
    person_requests::seed(&server, &person, &target).await;
    let body = escaped_person_request(&task);
    assert!(body.len() > 4 * 1024 * 1024);
    let response = raw(
        &server,
        &agent,
        "tools/call",
        padded_body(body, MCP_BODY_LIMIT),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let result = response.json::<Value>().await.unwrap()["result"].clone();
    assert_eq!(result["isError"], false, "{result}");
    assert_eq!(result["structuredContent"]["task_title"], "Large request");
    let stored = person_requests::read(&server, &person, "RQ-1").await;
    assert_eq!(stored["new_content"], "x".repeat(CONTENT_LIMIT));
    assert_eq!(stored["new_name"], "\u{1f601}".repeat(200));
    assert_eq!(stored["new_summary"], "\u{1f600}".repeat(500));
    for method in ["tools/call", "tools/list"] {
        let body = if method == "tools/call" {
            escaped_person_request(&task)
        } else {
            request_body(method, NEWEST_VERSION).to_string()
        };
        assert_eq!(
            raw(
                &server,
                &agent,
                method,
                padded_body(body, MCP_BODY_LIMIT + 1)
            )
            .await
            .status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
    let mut oversized = person_requests::creation("Too large");
    oversized["task_id"] = task["task_id"].clone();
    oversized["content"] = json!("x".repeat(CONTENT_LIMIT + 1));
    let error = call_error(&server, &agent, "create_request", oversized).await;
    assert_eq!(error["code"], "request.invalid_body");
    assert_eq!(error["errors"][0]["field"], "content");
    assert_eq!(
        list_page(&server, &person, PATH, &[]).await["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    server.stop().await;
}

#[tokio::test]
async fn call_list_person_requests_matches_keyword() {
    let (server, person, agent) = tasks::setup().await;
    person_requests::seed(&server, &person, "Nested/Needle").await;
    for body in [
        person_requests::creation("Needle name"),
        json!({"action":"create", "name":"Other", "summary":"Needle summary", "content":"Body"}),
        json!({"action":"change", "target_key":"KN-1", "content":"Body"}),
        json!({"action":"create", "name":"Unrelated", "summary":"Summary", "content":"Needle in content only"}),
    ] {
        person_requests::submit(&server, &person, body).await;
    }
    let mut arguments = json!({"space":SPACE, "keyword":"NEEDLE", "page_size":1});
    let mut rows = Vec::new();
    loop {
        let page = call(&server, &agent, "list_requests", arguments.clone()).await;
        let mut query = vec![("keyword", "NEEDLE"), ("page_size", "1")];
        if let Some(cursor) = arguments["cursor"].as_str() {
            query.push(("cursor", cursor));
        }
        assert_eq!(page, list_page(&server, &person, PATH, &query).await);
        assert!(page["data"][0].get("new_content").is_none());
        rows.push(page["data"][0]["key"].clone());
        let Some(cursor) = page["meta"]["next_cursor"].as_str() else {
            break;
        };
        assert!(rows.len() < 3);
        arguments["cursor"] = json!(cursor);
    }
    assert_eq!(rows, vec![json!("RQ-3"), json!("RQ-2"), json!("RQ-1")]);
    assert_invalid_filters(&server, &agent).await;
    server.stop().await;
}

#[tokio::test]
async fn call_list_person_requests_returns_only_matching_status() {
    let (server, person, agent) = tasks::setup().await;
    for name in ["Pending guide", "Approved guide", "Rejected guide"] {
        person_requests::submit(&server, &person, person_requests::creation(name)).await;
    }
    person_requests::approve(&server, &person, "RQ-2").await;
    person_requests::reject(&server, &person, "RQ-3").await;
    for (status, key) in [
        ("pending", "RQ-1"),
        ("approved", "RQ-2"),
        ("rejected", "RQ-3"),
    ] {
        let result = call(
            &server,
            &agent,
            "list_requests",
            json!({"space":SPACE, "status":status, "keyword":"guide"}),
        )
        .await;
        assert_eq!(
            result,
            list_page(
                &server,
                &person,
                PATH,
                &[("status", status), ("keyword", "guide")]
            )
            .await
        );
        assert_eq!(result["data"].as_array().unwrap().len(), 1);
        assert_eq!(result["data"][0]["key"], key);
        assert_eq!(result["data"][0]["status"], status);
    }
    server.stop().await;
}

#[tokio::test]
async fn call_get_person_request_returns_rejection_reason() {
    let (server, person, agent) = tasks::setup().await;
    person_requests::seed(&server, &person, "Guide").await;
    person_requests::submit(
        &server,
        &person,
        json!({"action":"change", "target_key":"KN-1", "content":"Proposed content"}),
    )
    .await;
    let arguments = json!({"space":SPACE, "key":"RQ-1"});
    let pending = call(&server, &agent, "get_request", arguments.clone()).await;
    assert_eq!(pending["changed_since_requested"], false);
    person_requests::change_entry(&server, &person, "KN-1", json!({"content":"Direct change"}))
        .await;
    let changed = call(&server, &agent, "get_request", arguments.clone()).await;
    assert_eq!(changed["changed_since_requested"], true);
    person_requests::reject(&server, &person, "RQ-1").await;
    let rejected = call(&server, &agent, "get_request", arguments).await;
    assert_eq!(
        rejected,
        person_requests::read(&server, &person, "RQ-1").await
    );
    assert_eq!(rejected["new_content"], "Proposed content");
    assert_eq!(rejected["status"], "rejected");
    assert_eq!(rejected["reject_reason"], "Superseded");
    assert_eq!(rejected["decided_by"], "alice");
    assert!(rejected.get("changed_since_requested").is_none());
    assert_eq!(
        call_error(
            &server,
            &agent,
            "get_request",
            json!({"space":SPACE, "key":"RQ-99"})
        )
        .await["code"],
        "person_request.not_found"
    );
    assert_eq!(
        call_error(
            &server,
            &agent,
            "get_request",
            json!({"space":SPACE, "key":"RQ-1", "unknown":"refused"})
        )
        .await["code"],
        "request.invalid_body"
    );
    server.stop().await;
}

const MCP_BODY_LIMIT: usize = 6 * CONTENT_LIMIT + 12 * (200 + 500 + 200) + 6 + 4096;

async fn assert_invalid_person_requests(server: &Server, agent: &Value, task: &Value) {
    for body in [
        json!({"action":"invalid"}),
        json!({"action":"create", "name":"Missing content", "summary":"Summary"}),
        json!({"action":"create", "name":"", "summary":"Summary", "content":"Content"}),
        json!({"action":"create", "name":"x".repeat(201), "summary":"Summary", "content":"Content"}),
        json!({"action":"create", "name":"Guide", "summary":"x".repeat(501), "content":"Content"}),
        json!({"action":"create", "name":"Guide", "summary":"Summary", "content":""}),
        json!({"action":"create", "name":"Guide", "summary":"Summary", "content":false}),
        json!({"action":"create", "name":"Guide", "summary":"Summary", "content":"Content", "target_key":"KN-1"}),
        json!({"action":"change", "target_key":"KN-1"}),
        json!({"action":"change", "target_key":"KN-1", "target_name":"Existing", "content":"Content"}),
        json!({"action":"delete"}),
        json!({"action":"delete", "target_key":"KN-1", "content":"Content"}),
        json!({"action":"delete", "target_key":"KN-1", "space":SPACE}),
        json!({"action":"delete", "target_key":"KN-1", "unknown":"refused"}),
    ] {
        let mut arguments = body;
        arguments["task_id"] = task["task_id"].clone();
        assert_eq!(
            call_error(server, agent, "create_request", arguments).await["code"],
            "request.invalid_body"
        );
    }
}

async fn assert_invalid_filters(server: &Server, agent: &Value) {
    for (extra, field) in [
        (json!({"keyword":""}), Some("keyword")),
        (json!({"keyword":"x".repeat(201)}), Some("keyword")),
        (json!({"status":"invalid"}), Some("status")),
        (json!({"page_size":0}), None),
        (json!({"page_size":101}), None),
        (json!({"cursor":"invalid"}), None),
        (json!({"status":false}), None),
        (json!({"unknown":"refused"}), None),
    ] {
        let mut arguments = json!({"space":SPACE});
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let error = call_error(server, agent, "list_requests", arguments).await;
        assert!(
            error["code"]
                .as_str()
                .unwrap()
                .starts_with("request.invalid_")
        );
        if let Some(field) = field {
            assert_eq!(error["code"], "request.invalid_query");
            assert_eq!(error["errors"][0]["field"], field);
        }
    }
}

fn escaped_person_request(task: &Value) -> String {
    let mut body = request_body("tools/call", NEWEST_VERSION);
    body["params"]["name"] = json!("create_request");
    body["params"]["arguments"] = json!({"task_id":task["task_id"], "action":"change", "target_name":"TARGET", "name":"NAME", "summary":"SUMMARY", "content":"CONTENT"});
    body.to_string()
        .replace(
            "\"TARGET\"",
            &format!("\"{}\"", "\\ud83d\\ude00".repeat(200)),
        )
        .replace("\"NAME\"", &format!("\"{}\"", "\\ud83d\\ude01".repeat(200)))
        .replace(
            "\"SUMMARY\"",
            &format!("\"{}\"", "\\ud83d\\ude00".repeat(500)),
        )
        .replace(
            "\"CONTENT\"",
            &format!("\"{}\"", "\\u0078".repeat(CONTENT_LIMIT)),
        )
}

async fn raw(server: &Server, agent: &Value, method: &str, body: String) -> Response {
    let mut request = server
        .client
        .post(format!("http://{}/mcp", server.address))
        .bearer_auth(agent["access_token"].as_str().unwrap())
        .header("content-type", "application/json")
        .header("accept", ACCEPT_TYPES)
        .header(VERSION_HEADER, NEWEST_VERSION)
        .header("mcp-method", method);
    if method == "tools/call" {
        request = request.header("mcp-name", "create_request");
    }
    request.body(body).send().await.unwrap()
}
