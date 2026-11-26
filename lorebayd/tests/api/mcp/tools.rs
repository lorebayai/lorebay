use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, authorize_for, create_space, list_page, login, request};
use crate::tasks::helpers::{self as tasks, SPACE, TASKS};

use super::{
    ACCEPT_TYPES, NEWEST_VERSION, SESSION_HEADER, VERSION_HEADER, notify_initialized, request_body,
};

mod agent_files;
mod body_errors;
mod filters;
mod knowledge;
mod person_requests;
mod task_lifecycle;
mod task_review;
mod update;

#[tokio::test]
async fn call_create_task_fails_with_error_result_when_argument_is_unknown() {
    let (server, person, agent) = tasks::setup().await;
    for body in [
        json!({"title":"Valid", "unknown":"do not echo this"}),
        json!({"title":""}),
        json!({"title":"   "}),
        json!({"title":"x".repeat(201)}),
        json!({"title":false}),
        json!({}),
    ] {
        let mut arguments = body.clone();
        arguments["space"] = json!(SPACE);
        let error = call_error(&server, &agent, "create_task", arguments).await;
        let response = request(&server, &agent, Method::POST, TASKS, body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let problem = response.json::<Value>().await.unwrap();
        assert_eq!(error["code"], problem["code"]);
        assert_eq!(error["message"], problem["title"]);
        assert_eq!(error["errors"], problem["errors"]);
        assert!(!error.to_string().contains("do not echo this"));
    }
    assert_eq!(
        list_page(&server, &person, TASKS, &[]).await["data"],
        json!([])
    );
    server.stop().await;
}

#[tokio::test]
async fn call_list_spaces_returns_spaces_with_capped_role() {
    let (server, person, agent) = tasks::setup().await;
    create_space(&server, &person, "Second", "Second team").await;
    create_space(&server, &person, "Third", "Third team").await;
    let mut rows = Vec::new();
    let mut arguments = json!({"page_size":1});
    loop {
        let page = call(&server, &agent, "list_spaces", arguments.clone()).await;
        let mut query = vec![("page_size", "1")];
        if let Some(cursor) = arguments["cursor"].as_str() {
            query.push(("cursor", cursor));
        }
        assert_eq!(
            page,
            list_page(&server, &agent, "/api/v1/spaces", &query).await
        );
        assert_eq!(page["data"][0]["role"], "contributor");
        assert_eq!(page["data"][0]["caller_person"], "alice");
        rows.push(page["data"][0]["key"].clone());
        if let Some(cursor) = page["meta"]["next_cursor"].as_str() {
            assert_ne!(arguments["cursor"], cursor);
            arguments["cursor"] = json!(cursor);
        } else {
            break;
        }
        assert!(rows.len() < 3);
    }
    assert_eq!(rows.len(), 3);
    rows.sort_by_key(Value::to_string);
    assert_eq!(
        rows,
        vec![json!("SPACE-1"), json!("SPACE-2"), json!("SPACE-3")]
    );
    for arguments in [
        json!({"page_size":0}),
        json!({"page_size":101}),
        json!({"page_size":-1}),
        json!({"page_size":1.5}),
        json!({"cursor":false}),
        json!({"cursor":"invalid"}),
    ] {
        assert_eq!(
            call_error(&server, &agent, "list_spaces", arguments).await["code"],
            "request.invalid_query"
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn call_create_task_returns_task_id_and_key() {
    let (server, person, agent) = tasks::setup().await;
    let title = "Research #KN-999";
    let created = call(
        &server,
        &agent,
        "create_task",
        json!({"space":SPACE, "title":title}),
    )
    .await;
    assert_eq!(created["key"], "TK-1");
    assert_eq!(created["task_title"], title);
    assert_eq!(created["task_id"].as_str().unwrap().len(), 26);
    let task = tasks::read(&server, &person, "TK-1").await;
    assert_eq!(task["task_id"], created["task_id"]);
    assert_eq!(task["title"], title);
    assert_eq!(task["agent_name"], "Research agent");
    assert_eq!(task["state"], "working");
    assert_eq!(
        call(
            &server,
            &agent,
            "get_task",
            json!({"space":SPACE, "key":"TK-1"})
        )
        .await,
        task
    );
    server.stop().await;
}

#[tokio::test]
async fn call_add_checkpoint_returns_task_title() {
    let (server, person, agent) = tasks::setup().await;
    let peer = authorize_for(&server, &person, "Other agent in same party").await;
    for version in [NEWEST_VERSION, "2025-03-26", "2025-06-18", "2025-11-25"] {
        if version != NEWEST_VERSION {
            initialize(&server, &peer, version).await;
        }
        let created = call_version(
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
            json!({"task_id":created["task_id"]}),
        )
        .await;
        let result = call_version(
            &server,
            &peer,
            "add_checkpoint",
            json!({"task_id":created["task_id"], "content":"Work recorded"}),
            version,
        )
        .await;
        assert_eq!(result["task_title"], version);
        assert!(result["time"].as_i64().unwrap() > 0);
        let key = created["key"].as_str().unwrap();
        assert_eq!(
            tasks::scratchpad(&server, &person, key).await,
            json!([{"time":result["time"], "agent_name":"Other agent in same party", "content":"Work recorded"}])
        );
        assert_eq!(
            tasks::read(&server, &person, key).await["last_activity_at"],
            result["time"]
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn call_add_checkpoint_fails_when_content_is_invalid() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Valid title").await;
    for content in [String::new(), "x".repeat(10_001)] {
        let error = call_error(
            &server,
            &agent,
            "add_checkpoint",
            json!({"task_id":task["task_id"], "content":content}),
        )
        .await;
        let response = tasks::checkpoint_request(
            &server,
            &agent,
            SPACE,
            task["task_id"].as_str().unwrap(),
            &content,
        )
        .await;
        let problem = response.json::<Value>().await.unwrap();
        assert_eq!(error["code"], problem["code"]);
        assert_eq!(error["errors"], problem["errors"]);
    }
    assert_eq!(tasks::scratchpad(&server, &person, "TK-1").await, json!([]));
    server.stop().await;
}

#[tokio::test]
async fn call_add_checkpoint_fails_when_task_id_does_not_exist() {
    let (server, person, _) = tasks::setup().await;
    let bob = login(&server, "bob").await;
    tasks::space_member(&server, &person, Method::POST, "contributor").await;
    let agent = authorize_for(&server, &bob, "Bob agent").await;
    let task = tasks::create(&server, &agent, SPACE, "Bob task").await;
    let unknown = call_error(
        &server,
        &agent,
        "add_checkpoint",
        json!({"task_id":"unknown", "content":"Forbidden"}),
    )
    .await;
    assert_eq!(unknown["code"], "task.not_found");
    for id in [
        "TK-1".to_owned(),
        task["task_id"].as_str().unwrap().to_uppercase(),
    ] {
        assert_eq!(
            call_error(
                &server,
                &agent,
                "add_checkpoint",
                json!({"task_id":id, "content":"Forbidden"})
            )
            .await,
            unknown
        );
    }
    tasks::space_member(&server, &person, Method::DELETE, "").await;
    assert_eq!(
        call_error(
            &server,
            &agent,
            "add_checkpoint",
            json!({"task_id":task["task_id"], "content":"Forbidden"})
        )
        .await,
        unknown
    );
    assert_eq!(tasks::scratchpad(&server, &person, "TK-1").await, json!([]));
    server.stop().await;
}

#[tokio::test]
async fn call_add_checkpoint_fails_when_task_belongs_to_another_party() {
    let (server, person, agent) = tasks::setup().await;
    let bob = login(&server, "bob").await;
    let bob_agent = authorize_for(&server, &bob, "Bob agent").await;
    let task = tasks::create(&server, &agent, SPACE, "Alice task").await;
    let arguments = json!({"task_id":task["task_id"], "content":"Forbidden"});
    assert_eq!(
        call_error(&server, &bob_agent, "add_checkpoint", arguments.clone()).await["code"],
        "task.not_found"
    );
    tasks::space_member(&server, &person, Method::POST, "contributor").await;
    assert_eq!(
        call_error(&server, &bob_agent, "add_checkpoint", arguments).await["code"],
        "space.permission_required"
    );
    let own = tasks::create(&server, &bob_agent, SPACE, "Bob task").await;
    tasks::space_member(&server, &person, Method::PATCH, "viewer").await;
    assert_eq!(
        call_error(
            &server,
            &bob_agent,
            "add_checkpoint",
            json!({"task_id":own["task_id"], "content":"Forbidden"})
        )
        .await["code"],
        "space.permission_required"
    );
    assert_eq!(tasks::scratchpad(&server, &person, "TK-1").await, json!([]));
    assert_eq!(tasks::scratchpad(&server, &person, "TK-2").await, json!([]));
    server.stop().await;
}

#[tokio::test]
async fn call_list_tasks_returns_tasks_with_reusable_ids() {
    let (server, person, agent) = tasks::setup().await;
    for title in ["First", "Second", "Third"] {
        tasks::create(&server, &agent, SPACE, title).await;
    }
    let mut arguments = json!({"space":SPACE, "page_size":1});
    let mut rows = Vec::new();
    loop {
        let page = call(&server, &agent, "list_tasks", arguments.clone()).await;
        rows.extend(page["data"].as_array().unwrap().iter().cloned());
        let mut query = vec![("page_size", "1")];
        if let Some(cursor) = arguments["cursor"].as_str() {
            query.push(("cursor", cursor));
        }
        assert_eq!(page, list_page(&server, &agent, TASKS, &query).await);
        if let Some(cursor) = page["meta"]["next_cursor"].as_str() {
            arguments["cursor"] = json!(cursor);
        } else {
            break;
        }
        assert!(rows.len() < 3);
    }
    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows.iter()
            .map(|row| row["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Third", "Second", "First"]
    );
    let checkpoint = call(
        &server,
        &agent,
        "add_checkpoint",
        json!({"task_id":rows[0]["task_id"], "content":"Recovered ID"}),
    )
    .await;
    assert_eq!(checkpoint["task_title"], "Third");
    assert_eq!(
        tasks::scratchpad(&server, &person, "TK-3").await[0]["content"],
        "Recovered ID"
    );
    server.stop().await;
}

#[tokio::test]
async fn call_get_task_returns_scratchpad_page_when_requested() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Paged checkpoints").await;
    let mut expected = Vec::new();
    for content in ["First", "Second", "Third", "Fourth", "Fifth"] {
        let added = tasks::append(&server, &agent, &task, content).await;
        expected
            .push(json!({"time":added["time"], "agent_name":"Research agent", "content":content}));
    }
    let mut arguments =
        json!({"space":SPACE, "key":"TK-1", "include_scratchpad":true, "page_size":2});
    let mut rows = Vec::new();
    loop {
        let page = call(&server, &agent, "get_task", arguments.clone()).await;
        let mut query = vec![("include_scratchpad", "true"), ("page_size", "2")];
        if let Some(cursor) = arguments["cursor"].as_str() {
            query.push(("cursor", cursor));
        }
        assert_eq!(
            page,
            list_page(&server, &person, &format!("{TASKS}/TK-1"), &query).await["data"]
        );
        rows.extend(
            page["scratchpad"]["data"]
                .as_array()
                .unwrap()
                .iter()
                .cloned(),
        );
        if let Some(cursor) = page["scratchpad"]["meta"]["next_cursor"].as_str() {
            arguments["cursor"] = json!(cursor);
        } else {
            break;
        }
        assert!(rows.len() < 5);
    }
    assert_eq!(rows, expected);
    for field in ["page_size", "cursor"] {
        let mut arguments = json!({"space":SPACE, "key":"TK-1"});
        arguments[field] = if field == "page_size" {
            json!(1)
        } else {
            json!("invalid")
        };
        let error = call_error(&server, &agent, "get_task", arguments).await;
        assert_eq!(error["code"], "request.invalid_query");
        assert_eq!(error["errors"][0]["field"], field);
    }
    server.stop().await;
}

pub(crate) async fn call(server: &Server, tokens: &Value, name: &str, arguments: Value) -> Value {
    call_version(server, tokens, name, arguments, NEWEST_VERSION).await
}

async fn call_version(
    server: &Server,
    tokens: &Value,
    name: &str,
    arguments: Value,
    version: &str,
) -> Value {
    let result = invoke(server, tokens, name, arguments, version).await;
    assert_eq!(result["isError"], false, "{result}");
    result["structuredContent"].clone()
}

pub(crate) async fn call_error(
    server: &Server,
    tokens: &Value,
    name: &str,
    arguments: Value,
) -> Value {
    let result = invoke(server, tokens, name, arguments, NEWEST_VERSION).await;
    assert_eq!(result["isError"], true, "{result}");
    result["structuredContent"].clone()
}

async fn invoke(
    server: &Server,
    tokens: &Value,
    name: &str,
    arguments: Value,
    version: &str,
) -> Value {
    let mut body = request_body("tools/call", version);
    body["params"]["name"] = json!(name);
    body["params"]["arguments"] = arguments;
    let response = server
        .client
        .post(format!("http://{}/mcp", server.address))
        .bearer_auth(tokens["access_token"].as_str().unwrap())
        .header("accept", ACCEPT_TYPES)
        .header(VERSION_HEADER, version)
        .header("mcp-method", "tools/call")
        .header("mcp-name", name)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    assert!(!response.headers().contains_key(SESSION_HEADER));
    let text = response.text().await.unwrap();
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: Value = serde_json::from_str(&text).unwrap();
    assert!(body.get("error").is_none(), "{body}");
    let result = &body["result"];
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        result["structuredContent"]
    );
    if version == NEWEST_VERSION {
        assert_eq!(result["resultType"], "complete");
    } else {
        assert!(result.get("resultType").is_none());
    }
    result.clone()
}

async fn initialize(server: &Server, tokens: &Value, version: &str) {
    let response = server.client.post(format!("http://{}/mcp", server.address))
        .bearer_auth(tokens["access_token"].as_str().unwrap()).header("accept", ACCEPT_TYPES)
        .json(&json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{
            "protocolVersion":version, "capabilities":{}, "clientInfo":{"name":"integration", "version":"1"}
        }})).send().await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert!(!response.headers().contains_key(SESSION_HEADER));
    assert_eq!(
        response.json::<Value>().await.unwrap()["result"]["protocolVersion"],
        version
    );
    notify_initialized(server, version, tokens["access_token"].as_str().unwrap()).await;
}
