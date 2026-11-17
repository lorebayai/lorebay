use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::super::helpers::{Server, TEST_PASSWORD, list_response};

pub const SPACE: &str = "/api/v1/spaces/SPACE-1";
pub const PARTICIPANTS: &str = "/api/v1/spaces/SPACE-1/participants";
pub const USERS: &[(&str, &str)] = &[
    ("alice", TEST_PASSWORD),
    ("bob", TEST_PASSWORD),
    ("carol", TEST_PASSWORD),
    ("dave", TEST_PASSWORD),
];

pub async fn setup() -> (Server, Value) {
    let server = Server::restart_with_users(TempDir::new().unwrap(), USERS).await;
    let owner = server.tokens().await;
    create_space(&server, &owner).await;
    (server, owner)
}

pub async fn login(server: &Server, username: &str) -> Value {
    let response = server.log_in(username, TEST_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["data"].clone()
}

pub async fn request(
    server: &Server,
    tokens: &Value,
    method: Method,
    path: &str,
    body: Value,
) -> Response {
    server
        .json(
            method,
            path,
            body,
            Some(tokens["access_token"].as_str().unwrap()),
        )
        .await
}

pub async fn create_space(server: &Server, tokens: &Value) {
    let response = request(
        server,
        tokens,
        Method::POST,
        "/api/v1/spaces",
        json!({"name":"Team", "summary":"Shared work"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

pub async fn mutation(
    server: &Server,
    tokens: &Value,
    method: Method,
    username: &str,
    role: &str,
) -> Response {
    request(
        server,
        tokens,
        method,
        &format!("{PARTICIPANTS}/{username}"),
        json!({"role":role}),
    )
    .await
}

pub async fn add(server: &Server, tokens: &Value, username: &str, role: &str) {
    let response = mutation(server, tokens, Method::POST, username, role).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data":{"type":"human", "username":username, "role":role}, "meta":{}})
    );
}

pub async fn change(server: &Server, tokens: &Value, username: &str, role: &str) {
    let response = mutation(server, tokens, Method::PATCH, username, role).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data":{"type":"human", "username":username, "role":role}, "meta":{}})
    );
}

pub async fn authorize_for(server: &Server, tokens: &Value, name: &str) -> Value {
    let client = server.register_client(name).await;
    let code = server
        .approve_code_for(
            &client,
            Some(name),
            tokens["access_token"].as_str().unwrap(),
        )
        .await;
    let response = server.exchange_code(&client, &code).await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

pub async fn list(server: &Server, tokens: &Value, path: &str) -> Value {
    let response = list_response(path, server, tokens, &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["data"].clone()
}

pub async fn assert_role(server: &Server, tokens: &Value, role: &str) {
    let response = request(server, tokens, Method::GET, SPACE, Value::Null).await;
    assert_eq!(response.status(), StatusCode::OK);
    let space = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(space["role"], role);
    assert_eq!(space["key"], "SPACE-1");
    assert_eq!(list(server, tokens, "/api/v1/spaces").await, json!([space]));
}

pub async fn assert_target_role(server: &Server, tokens: &Value, username: &str, role: &str) {
    let roster = list(server, tokens, PARTICIPANTS).await;
    let person = roster
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["username"] == username && row["type"] == "human")
        .unwrap();
    assert_eq!(
        person,
        &json!({"username":username, "type":"human", "role":role})
    );
}

pub async fn walk(server: &Server, tokens: &Value, path: &str, size: &str) -> Vec<Value> {
    let mut rows = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut query = vec![("page_size", size)];
        if let Some(cursor) = &cursor {
            query.push(("cursor", cursor));
        }
        let response = list_response(path, server, tokens, &query).await;
        assert_eq!(response.status(), StatusCode::OK);
        let page = response.json::<Value>().await.unwrap();
        let items = page["data"].as_array().unwrap();
        assert!(items.len() <= size.parse::<usize>().unwrap());
        rows.extend(items.iter().cloned());
        assert!(rows.len() < 100);
        match page["meta"]["next_cursor"].as_str() {
            Some(next) => {
                assert_eq!(items.len(), size.parse::<usize>().unwrap());
                assert_ne!(cursor.as_deref(), Some(next));
                cursor = Some(next.to_owned());
            }
            None => break,
        }
    }
    rows
}
