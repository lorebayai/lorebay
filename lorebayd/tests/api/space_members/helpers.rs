use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::super::helpers::{Server, TEST_PASSWORD, create_space, list, request};

pub const SPACE: &str = "/api/v1/spaces/SPACE-1";
pub const SPACE_MEMBERS: &str = "/api/v1/spaces/SPACE-1/members";
pub const USERS: &[(&str, &str)] = &[
    ("alice", TEST_PASSWORD),
    ("bob", TEST_PASSWORD),
    ("carol", TEST_PASSWORD),
    ("dave", TEST_PASSWORD),
];

pub async fn setup() -> (Server, Value) {
    let server = Server::restart_with_users(TempDir::new().unwrap(), USERS).await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared work").await;
    (server, owner)
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
        &format!("{SPACE_MEMBERS}/{username}"),
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

pub async fn assert_role(server: &Server, tokens: &Value, role: &str) {
    let response = request(server, tokens, Method::GET, SPACE, Value::Null).await;
    assert_eq!(response.status(), StatusCode::OK);
    let space = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(space["role"], role);
    assert_eq!(space["key"], "SPACE-1");
    assert_eq!(list(server, tokens, "/api/v1/spaces").await, json!([space]));
}

pub async fn assert_target_role(server: &Server, tokens: &Value, username: &str, role: &str) {
    let roster = list(server, tokens, SPACE_MEMBERS).await;
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
