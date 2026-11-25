use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, create_space, list_page, request};

pub const PATH: &str = "/api/v1/spaces/SPACE-1/requests";
pub const KNOWLEDGE: &str = "/api/v1/spaces/SPACE-1/knowledge";
pub const BODY_LIMIT: usize = 6 * 1024 * 1024 + 12 * (200 + 500 + 200) + 6 + 1024;

pub async fn setup() -> (Server, Value) {
    let server = Server::executable().await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared").await;
    (server, owner)
}

pub async fn data(response: Response, status: StatusCode) -> Value {
    let observed = response.status();
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(observed, status, "{body}");
    body["data"].clone()
}

pub fn creation(name: &str) -> Value {
    json!({"action":"create", "name":name, "summary":"Summary", "content":"Content"})
}

pub fn by_key(key: &str) -> String {
    format!("{PATH}/{key}")
}

pub async fn submit(server: &Server, caller: &Value, body: Value) -> Value {
    data(
        request(server, caller, Method::POST, PATH, body).await,
        StatusCode::CREATED,
    )
    .await
}

pub async fn read(server: &Server, caller: &Value, key: &str) -> Value {
    list_page(server, caller, &by_key(key), &[]).await["data"].clone()
}

pub async fn approve(server: &Server, caller: &Value, key: &str) -> Value {
    data(
        request(
            server,
            caller,
            Method::POST,
            &format!("{}/approve", by_key(key)),
            json!({"revision":1}),
        )
        .await,
        StatusCode::OK,
    )
    .await
}

pub async fn reject(server: &Server, caller: &Value, key: &str) -> Value {
    data(
        request(
            server,
            caller,
            Method::POST,
            &format!("{}/reject", by_key(key)),
            json!({"reason":"Superseded","revision":1}),
        )
        .await,
        StatusCode::OK,
    )
    .await
}

pub async fn seed(server: &Server, owner: &Value, name: &str) -> Value {
    let mut body = creation(name);
    body.as_object_mut().unwrap().remove("action");
    data(
        request(server, owner, Method::POST, KNOWLEDGE, body).await,
        StatusCode::CREATED,
    )
    .await
}

pub async fn entry(server: &Server, caller: &Value, key: &str) -> Value {
    list_page(server, caller, &format!("{KNOWLEDGE}/by-key/{key}"), &[]).await["data"].clone()
}

pub fn task_path(task: &Value) -> String {
    format!(
        "/api/v1/spaces/SPACE-1/tasks/{}/requests",
        task["task_id"].as_str().unwrap()
    )
}

pub async fn submit_from_task(server: &Server, agent: &Value, task: &Value) -> Value {
    data(
        request(
            server,
            agent,
            Method::POST,
            &task_path(task),
            creation("Agent guide"),
        )
        .await,
        StatusCode::CREATED,
    )
    .await
}

pub async fn raw(server: &Server, caller: &Value, path: &str, body: String) -> Response {
    server
        .client
        .post(format!("http://{}{path}", server.address))
        .bearer_auth(caller["access_token"].as_str().unwrap())
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap()
}

pub async fn submit_deleted_targets(server: &Server, owner: &Value, agent: &Value, task: &Value) {
    seed(server, owner, "Deleted guide").await;
    data(
        request(
            server,
            owner,
            Method::DELETE,
            &format!("{KNOWLEDGE}/by-key/KN-1?version=1"),
            Value::Null,
        )
        .await,
        StatusCode::OK,
    )
    .await;
    let result = data(
        request(
            server,
            agent,
            Method::POST,
            &task_path(task),
            json!({"action":"change","target_name":"deleted GUIDE","summary":"Restore summary"}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(result["base_version"], 2);
    assert_eq!(
        result["target"],
        json!({"key":"KN-1","name":"Deleted guide"})
    );
    assert_eq!(result["task_title"], "Improve guide");
    assert_eq!(
        read(server, owner, "RQ-2").await["new_summary"],
        "Restore summary"
    );
    assert_eq!(
        submit(
            server,
            owner,
            json!({"action":"delete","target_key":"KN-1"})
        )
        .await["base_version"],
        2
    );
}

pub async fn versions(server: &Server, caller: &Value, key: &str) -> Value {
    list_page(
        server,
        caller,
        &format!("{KNOWLEDGE}/by-key/{key}/versions"),
        &[("page_size", "100")],
    )
    .await["data"]
        .clone()
}

pub async fn old_entry(server: &Server, caller: &Value, key: &str, version: i64) -> Value {
    list_page(
        server,
        caller,
        &format!("{KNOWLEDGE}/by-key/{key}/versions/{version}"),
        &[],
    )
    .await["data"]
        .clone()
}

pub async fn change_entry(server: &Server, owner: &Value, key: &str, mut changes: Value) -> Value {
    changes["version"] = entry(server, owner, key).await["version"].clone();
    data(
        request(
            server,
            owner,
            Method::PATCH,
            &format!("{KNOWLEDGE}/by-key/{key}"),
            changes,
        )
        .await,
        StatusCode::OK,
    )
    .await
}

pub async fn delete_entry(server: &Server, owner: &Value, key: &str) -> Value {
    let version = entry(server, owner, key).await["version"].as_i64().unwrap();
    data(
        request(
            server,
            owner,
            Method::DELETE,
            &format!("{KNOWLEDGE}/by-key/{key}?version={version}"),
            Value::Null,
        )
        .await,
        StatusCode::OK,
    )
    .await
}

pub fn assert_version(
    version: &Value,
    number: i64,
    kind: &str,
    person: &str,
    person_request: Option<&str>,
) {
    assert_eq!(version["version"], number);
    assert_eq!(version["kind"], kind);
    assert_eq!(version["applied_by"], person);
    assert!(version["applied_at"].as_i64().unwrap() > 0);
    assert_eq!(version["request_key"], json!(person_request));
}

pub async fn revise(server: &Server, caller: &Value, key: &str, body: Value) -> Value {
    data(
        request(server, caller, Method::PATCH, &by_key(key), body).await,
        StatusCode::OK,
    )
    .await
}

pub async fn approve_revision(server: &Server, caller: &Value, key: &str, revision: i64) -> Value {
    data(
        request(
            server,
            caller,
            Method::POST,
            &format!("{}/approve", by_key(key)),
            json!({"revision":revision}),
        )
        .await,
        StatusCode::OK,
    )
    .await
}
