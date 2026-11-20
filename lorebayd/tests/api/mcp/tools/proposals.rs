use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, authorize_for, list_page, login};
use crate::knowledge::helpers::{CONTENT_LIMIT, padded_body};
use crate::proposals::helpers::{self as proposals, PATH};
use crate::tasks::helpers::{self as tasks, SPACE};

use super::super::{ACCEPT_TYPES, NEWEST_VERSION, VERSION_HEADER, request_body};
use super::{call, call_error};

#[tokio::test]
async fn call_create_proposal_returns_task_title_and_pending_proposal() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Improve guide").await;
    proposals::seed(&server, &person, "Existing").await;
    call(
        &server,
        &agent,
        "update_task",
        json!({"task_id":task["task_id"], "status":"ended"}),
    )
    .await;
    for (index, body) in [
        proposals::creation("New guide"),
        json!({"kind":"change", "target_name":"Existing", "name":"Renamed", "summary":"New summary", "content":"New content"}),
        json!({"kind":"delete", "target_key":"KN-1"}),
    ].into_iter().enumerate() {
        let mut arguments = body;
        arguments["task_id"] = task["task_id"].clone();
        let result = call(&server, &agent, "create_proposal", arguments).await;
        assert_eq!(result["task_title"], "Improve guide");
        assert_eq!(result["status"], "pending");
        assert_eq!(result["proposer_agent_name"], "Research agent");
        assert_eq!(result["proposed_by"], "alice");
        assert_eq!(result["from_task_key"], "TK-1");
        let key = format!("PP-{}", index + 1);
        let mut read = proposals::read(&server, &person, &key).await;
        read["task_title"] = json!("Improve guide");
        assert_eq!(result, read);
    }
    assert_eq!(
        proposals::entry(&server, &person, "KN-1").await["name"],
        "Existing"
    );
    assert_eq!(
        tasks::read(&server, &person, "TK-1").await["status"],
        "ended"
    );
    assert_invalid_proposals(&server, &agent, &task).await;
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
async fn call_create_proposal_fails_when_task_belongs_to_another_party() {
    let (server, person, agent) = tasks::setup().await;
    tasks::participant(&server, &person, Method::POST, "contributor").await;
    let bob = login(&server, "bob").await;
    let other = authorize_for(&server, &bob, "Other party").await;
    let task = tasks::create(&server, &agent, SPACE, "Private write target").await;
    let mut arguments = proposals::creation("Forbidden");
    arguments["task_id"] = task["task_id"].clone();
    assert_eq!(
        call_error(&server, &other, "create_proposal", arguments).await["code"],
        "space.permission_required"
    );
    assert_eq!(
        list_page(&server, &person, PATH, &[]).await["data"],
        json!([])
    );
    server.stop().await;
}

#[tokio::test]
async fn call_create_proposal_accepts_content_at_size_limit() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Large proposal").await;
    let target = "\u{1f600}".repeat(200);
    proposals::seed(&server, &person, &target).await;
    let body = escaped_proposal(&task);
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
    assert_eq!(result["structuredContent"]["task_title"], "Large proposal");
    let stored = proposals::read(&server, &person, "PP-1").await;
    assert_eq!(stored["new_content"], "x".repeat(CONTENT_LIMIT));
    assert_eq!(stored["new_name"], "\u{1f601}".repeat(200));
    assert_eq!(stored["new_summary"], "\u{1f600}".repeat(500));
    for method in ["tools/call", "tools/list"] {
        let body = if method == "tools/call" {
            escaped_proposal(&task)
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
    let mut oversized = proposals::creation("Too large");
    oversized["task_id"] = task["task_id"].clone();
    oversized["content"] = json!("x".repeat(CONTENT_LIMIT + 1));
    let error = call_error(&server, &agent, "create_proposal", oversized).await;
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
async fn call_list_proposals_matches_keyword() {
    let (server, person, agent) = tasks::setup().await;
    proposals::seed(&server, &person, "Nested/Needle").await;
    for body in [
        proposals::creation("Needle name"),
        json!({"kind":"create", "name":"Other", "summary":"Needle summary", "content":"Body"}),
        json!({"kind":"change", "target_key":"KN-1", "content":"Body"}),
        json!({"kind":"create", "name":"Unrelated", "summary":"Summary", "content":"Needle in content only"}),
    ] {
        proposals::submit(&server, &person, body).await;
    }
    let mut arguments = json!({"space":SPACE, "keyword":"NEEDLE", "page_size":1});
    let mut rows = Vec::new();
    loop {
        let page = call(&server, &agent, "list_proposals", arguments.clone()).await;
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
    assert_eq!(rows, vec![json!("PP-3"), json!("PP-2"), json!("PP-1")]);
    assert_invalid_filters(&server, &agent).await;
    server.stop().await;
}

#[tokio::test]
async fn call_list_proposals_returns_only_matching_status() {
    let (server, person, agent) = tasks::setup().await;
    for name in ["Pending guide", "Approved guide", "Rejected guide"] {
        proposals::submit(&server, &person, proposals::creation(name)).await;
    }
    proposals::approve(&server, &person, "PP-2").await;
    proposals::reject(&server, &person, "PP-3").await;
    for (status, key) in [
        ("pending", "PP-1"),
        ("approved", "PP-2"),
        ("rejected", "PP-3"),
    ] {
        let result = call(
            &server,
            &agent,
            "list_proposals",
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
async fn call_get_proposal_returns_rejection_reason() {
    let (server, person, agent) = tasks::setup().await;
    proposals::seed(&server, &person, "Guide").await;
    proposals::submit(
        &server,
        &person,
        json!({"kind":"change", "target_key":"KN-1", "content":"Proposed content"}),
    )
    .await;
    let arguments = json!({"space":SPACE, "key":"PP-1"});
    let pending = call(&server, &agent, "get_proposal", arguments.clone()).await;
    assert_eq!(pending["changed_since_proposed"], false);
    proposals::change_entry(&server, &person, "KN-1", json!({"content":"Direct change"})).await;
    let changed = call(&server, &agent, "get_proposal", arguments.clone()).await;
    assert_eq!(changed["changed_since_proposed"], true);
    proposals::reject(&server, &person, "PP-1").await;
    let rejected = call(&server, &agent, "get_proposal", arguments).await;
    assert_eq!(rejected, proposals::read(&server, &person, "PP-1").await);
    assert_eq!(rejected["new_content"], "Proposed content");
    assert_eq!(rejected["status"], "rejected");
    assert_eq!(rejected["reject_reason"], "Superseded");
    assert_eq!(rejected["decided_by"], "alice");
    assert!(rejected.get("changed_since_proposed").is_none());
    assert_eq!(
        call_error(
            &server,
            &agent,
            "get_proposal",
            json!({"space":SPACE, "key":"PP-99"})
        )
        .await["code"],
        "proposal.not_found"
    );
    assert_eq!(
        call_error(
            &server,
            &agent,
            "get_proposal",
            json!({"space":SPACE, "key":"PP-1", "unknown":"refused"})
        )
        .await["code"],
        "request.invalid_body"
    );
    server.stop().await;
}

const MCP_BODY_LIMIT: usize = 6 * CONTENT_LIMIT + 12 * (200 + 500 + 200) + 6 + 4096;

async fn assert_invalid_proposals(server: &Server, agent: &Value, task: &Value) {
    for body in [
        json!({"kind":"invalid"}),
        json!({"kind":"create", "name":"Missing content", "summary":"Summary"}),
        json!({"kind":"create", "name":"", "summary":"Summary", "content":"Content"}),
        json!({"kind":"create", "name":"x".repeat(201), "summary":"Summary", "content":"Content"}),
        json!({"kind":"create", "name":"Guide", "summary":"x".repeat(501), "content":"Content"}),
        json!({"kind":"create", "name":"Guide", "summary":"Summary", "content":""}),
        json!({"kind":"create", "name":"Guide", "summary":"Summary", "content":false}),
        json!({"kind":"create", "name":"Guide", "summary":"Summary", "content":"Content", "target_key":"KN-1"}),
        json!({"kind":"change", "target_key":"KN-1"}),
        json!({"kind":"change", "target_key":"KN-1", "target_name":"Existing", "content":"Content"}),
        json!({"kind":"delete"}),
        json!({"kind":"delete", "target_key":"KN-1", "content":"Content"}),
        json!({"kind":"delete", "target_key":"KN-1", "space":SPACE}),
        json!({"kind":"delete", "target_key":"KN-1", "unknown":"refused"}),
    ] {
        let mut arguments = body;
        arguments["task_id"] = task["task_id"].clone();
        assert_eq!(
            call_error(server, agent, "create_proposal", arguments).await["code"],
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
        let error = call_error(server, agent, "list_proposals", arguments).await;
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

fn escaped_proposal(task: &Value) -> String {
    let mut body = request_body("tools/call", NEWEST_VERSION);
    body["params"]["name"] = json!("create_proposal");
    body["params"]["arguments"] = json!({"task_id":task["task_id"], "kind":"change", "target_name":"TARGET", "name":"NAME", "summary":"SUMMARY", "content":"CONTENT"});
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
        request = request.header("mcp-name", "create_proposal");
    }
    request.body(body).send().await.unwrap()
}
