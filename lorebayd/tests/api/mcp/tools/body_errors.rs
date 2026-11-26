use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, assert_problem, authorize_for, create_space, list_page, request};
use crate::person_requests::helpers::{PATH as REQUESTS, task_path};
use crate::tasks::helpers::{self as tasks, SPACE, TASKS};

use super::{NEWEST_VERSION, call_error, invoke};

#[tokio::test]
async fn create_task_names_invalid_body_fields() {
    let (server, person, agent) = setup().await;
    for (body, field, code, message) in [
        (json!({}), "title", "required", "`title` is required."),
        (
            json!({"title":false}),
            "title",
            "invalid",
            "`title` must be a string.",
        ),
        (
            json!({"title":"Valid", "planned":"private-sentinel"}),
            "planned",
            "invalid",
            "`planned` must be a boolean.",
        ),
        (
            json!({"title":"Valid", "unknown":"private-sentinel"}),
            "unknown",
            "invalid",
            "`unknown` is not accepted.",
        ),
        (
            json!({"title":"Valid", "a`b":"private-sentinel"}),
            "a`b",
            "invalid",
            "`a`b` is not accepted.",
        ),
    ] {
        let mut arguments = body.clone();
        arguments["space"] = json!(SPACE);
        let error = call_error(&server, &agent, "create_task", arguments).await;
        assert_error(&error, field, code, message);
        let response = request(&server, &agent, Method::POST, TASKS, body).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let problem = response.json::<Value>().await.unwrap();
        assert_eq!(problem["code"], error["code"]);
        assert_eq!(problem["title"], error["message"]);
        assert_eq!(problem["errors"], error["errors"]);
    }
    let error = call_error(
        &server,
        &agent,
        "create_task",
        json!({"planned":1, "unknown":true}),
    )
    .await;
    assert_eq!(
        error["errors"],
        json!([
            {"field":"planned", "code":"invalid", "message":"`planned` must be a boolean."},
            {"field":"unknown", "code":"invalid", "message":"`unknown` is not accepted."},
            {"field":"space", "code":"required", "message":"`space` is required."},
            {"field":"title", "code":"required", "message":"`title` is required."}
        ])
    );
    assert_eq!(
        list_page(&server, &person, TASKS, &[]).await["data"],
        json!([])
    );
    server.stop().await;
}

#[tokio::test]
async fn create_request_names_invalid_action_fields() {
    let (server, person, agent) = setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Original").await;
    let original = tasks::read(&server, &person, "TK-1").await;
    for (body, field, code, message) in [
        (json!({}), "action", "required", "`action` is required."),
        (
            json!({"action":"create", "summary":"Summary", "content":"Content"}),
            "name",
            "required",
            "`name` is required.",
        ),
        (
            json!({"action":"create", "name":"Guide", "content":"Content"}),
            "summary",
            "required",
            "`summary` is required.",
        ),
        (
            json!({"action":"create", "name":"Guide", "summary":"Summary"}),
            "content",
            "required",
            "`content` is required.",
        ),
        (
            json!({"action":"private-sentinel"}),
            "action",
            "invalid",
            "`action` must be one of `change`, `create`, `delete`.",
        ),
        (
            json!({"action":"create", "name":"Guide", "summary":"Summary", "content":"Content", "target_key":"private-sentinel"}),
            "target_key",
            "invalid",
            "`target_key` is not accepted.",
        ),
        (
            json!({"action":"delete", "target_key":"KN-1", "name":"private-sentinel"}),
            "name",
            "invalid",
            "`name` is not accepted.",
        ),
        (
            json!({"action":"delete", "target_key":"KN-1", "a`b":"private-sentinel"}),
            "a`b",
            "invalid",
            "`a`b` is not accepted.",
        ),
    ] {
        let error = request_errors(&server, &person, &agent, &task, body).await;
        assert_error(&error, field, code, message);
    }
    assert_refused_request_fields(&server, &agent, &task).await;
    assert_eq!(tasks::read(&server, &person, "TK-1").await, original);
    assert_eq!(
        list_page(&server, &person, REQUESTS, &[]).await["data"],
        json!([])
    );
    server.stop().await;
}

#[tokio::test]
async fn create_request_names_wrongly_typed_body_fields() {
    let (server, person, agent) = setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Original").await;
    let original = tasks::read(&server, &person, "TK-1").await;
    for (body, field) in [
        (json!({"action":5}), "action"),
        (json!({"action":false}), "action"),
        (json!({"action":{"private-sentinel":true}}), "action"),
        (
            json!({"action":"create", "name":5, "summary":"Summary", "content":"Content"}),
            "name",
        ),
        (
            json!({"action":"create", "name":{"private-sentinel":true}, "summary":"Summary", "content":"Content"}),
            "name",
        ),
        (json!({"action":"delete", "target_key":5}), "target_key"),
        (
            json!({"action":"delete", "target_key":["private-sentinel"]}),
            "target_key",
        ),
        (
            json!({"action":"change", "target_key":"KN-1", "summary":false}),
            "summary",
        ),
    ] {
        let error = request_errors(&server, &person, &agent, &task, body).await;
        assert_error(
            &error,
            field,
            "invalid",
            &format!("`{field}` must be a string."),
        );
    }
    assert_eq!(tasks::read(&server, &person, "TK-1").await, original);
    assert_eq!(
        list_page(&server, &person, REQUESTS, &[]).await["data"],
        json!([])
    );
    server.stop().await;
}

#[tokio::test]
async fn update_task_names_invalid_body_fields() {
    let (server, person, agent) = setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Original").await;
    let original = tasks::read(&server, &person, "TK-1").await;
    let path = format!("{TASKS}/{}", task["task_id"].as_str().unwrap());
    for changes in [
        json!({"title":"Must not apply", "result_version":0}),
        json!({"title":"Must not apply", "result_version":-1}),
        json!({"title":"Must not apply", "result_version":false}),
        json!({"title":"Must not apply", "result_version":"private-sentinel"}),
        json!({"title":"Must not apply", "result_version":1.5}),
        json!({"title":"Must not apply", "result_version":u64::MAX}),
    ] {
        let mut arguments = changes.clone();
        arguments["task_id"] = task["task_id"].clone();
        let error = call_error(&server, &agent, "update_task", arguments).await;
        assert_error(
            &error,
            "result_version",
            "invalid",
            "`result_version` must be a positive integer or null.",
        );
        let response = request(&server, &agent, Method::PATCH, &path, changes).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let problem = response.json::<Value>().await.unwrap();
        assert_eq!(problem["code"], error["code"]);
        assert_eq!(problem["errors"], error["errors"]);
    }
    let error = call_error(
        &server,
        &agent,
        "update_task",
        json!({"title":"Must not apply"}),
    )
    .await;
    assert_error(&error, "task_id", "required", "`task_id` is required.");
    assert_eq!(tasks::read(&server, &person, "TK-1").await, original);
    server.stop().await;
}

#[tokio::test]
async fn create_task_reports_body_when_json_is_not_object() {
    let (server, person, agent) = setup().await;
    for body in [
        json!([]),
        json!(false),
        json!("private-sentinel"),
        Value::Null,
    ] {
        let response = request(&server, &agent, Method::POST, TASKS, body.clone()).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let problem = response.json::<Value>().await.unwrap();
        assert_error(
            &problem,
            "body",
            "invalid",
            "Expected a JSON object with the required fields.",
        );
        let result = invoke(&server, &agent, "create_task", body, NEWEST_VERSION).await;
        assert_eq!(result["isError"], true);
        assert_error(
            &result["structuredContent"],
            "body",
            "invalid",
            "Expected a JSON object with the required fields.",
        );
    }
    let response = server
        .client
        .post(format!("http://{}{TASKS}", server.address))
        .bearer_auth(agent["access_token"].as_str().unwrap())
        .header("content-type", "application/json")
        .body("{\"title\":\"private-sentinel\"")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_error(
        &response.json::<Value>().await.unwrap(),
        "body",
        "invalid",
        "Expected a JSON object with the required fields.",
    );
    assert_eq!(
        list_page(&server, &person, TASKS, &[]).await["data"],
        json!([])
    );
    server.stop().await;
}

async fn request_errors(
    server: &Server,
    person: &Value,
    agent: &Value,
    task: &Value,
    body: Value,
) -> Value {
    let mut arguments = body.clone();
    arguments["task_id"] = task["task_id"].clone();
    let error = call_error(server, agent, "create_request", arguments).await;
    for (caller, path) in [(agent, task_path(task)), (person, REQUESTS.to_owned())] {
        let response = request(server, caller, Method::POST, &path, body.clone()).await;
        let problem =
            assert_problem(response, StatusCode::BAD_REQUEST, "request.invalid_body").await;
        assert_eq!(problem["code"], error["code"]);
        assert_eq!(problem["title"], error["message"]);
        assert_eq!(problem["errors"], error["errors"]);
    }
    error
}

async fn assert_refused_request_fields(server: &Server, agent: &Value, task: &Value) {
    let error = call_error(
        server,
        agent,
        "create_request",
        json!({"task_id":task["task_id"], "action":"delete", "target_key":"KN-1",
            "name":"private-sentinel", "summary":"private-sentinel", "content":"private-sentinel"}),
    )
    .await;
    assert_eq!(error["code"], "request.invalid_body");
    assert_eq!(
        error["errors"],
        json!([
            {"field":"name", "code":"invalid", "message":"`name` is not accepted."},
            {"field":"summary", "code":"invalid", "message":"`summary` is not accepted."},
            {"field":"content", "code":"invalid", "message":"`content` is not accepted."}
        ])
    );
    assert!(!error.to_string().contains("private-sentinel"));
}

fn assert_error(error: &Value, field: &str, code: &str, message: &str) {
    assert_eq!(error["code"], "request.invalid_body");
    assert_eq!(
        error["errors"],
        json!([{"field":field, "code":code, "message":message}])
    );
    assert!(!error.to_string().contains("private-sentinel"));
}

async fn setup() -> (Server, Value, Value) {
    let server = Server::executable().await;
    let person = server.tokens().await;
    create_space(&server, &person, "Team", "Shared work").await;
    let agent = authorize_for(&server, &person, "Research agent").await;
    (server, person, agent)
}
