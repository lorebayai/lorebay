use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use url::form_urlencoded::Serializer;

use crate::helpers::{Server, create_space, request};

pub const PATH: &str = "/api/v1/spaces/SPACE-1/knowledge";
pub const CONTENT_LIMIT: usize = 1024 * 1024;
pub const ENTRY_BODY_LIMIT: usize = 6 * CONTENT_LIMIT + 12 * (200 + 500 + 200) + 6 + 1024;

pub async fn setup() -> (Server, Value) {
    let server = Server::start().await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared work").await;
    (server, owner)
}

pub async fn data(response: Response, status: StatusCode) -> Value {
    let observed = response.status();
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(observed, status, "{body}");
    assert!(body["meta"].is_object());
    body["data"].clone()
}

pub async fn create(
    server: &Server,
    owner: &Value,
    name: &str,
    summary: &str,
    content: &str,
) -> Value {
    data(
        request(
            server,
            owner,
            Method::POST,
            PATH,
            json!({"name": name, "summary": summary, "content": content}),
        )
        .await,
        StatusCode::CREATED,
    )
    .await
}

pub async fn seed(server: &Server, owner: &Value, name: &str) -> Value {
    create(server, owner, name, "Summary", "Content").await
}

pub fn by_key(key: &str) -> String {
    format!("{PATH}/by-key/{key}")
}

pub fn by_name(name: &str) -> String {
    let query = Serializer::new(String::new())
        .append_pair("name", name)
        .finish();
    format!("{PATH}/entry?{query}")
}

pub fn padded_body(mut body: String, length: usize) -> String {
    body.push_str(&" ".repeat(length - body.len()));
    body
}

pub async fn read(server: &Server, caller: &Value, path: &str) -> Value {
    data(
        request(server, caller, Method::GET, path, Value::Null).await,
        StatusCode::OK,
    )
    .await
}

pub async fn change(server: &Server, owner: &Value, path: &str, changes: Value) -> Value {
    data(
        request(server, owner, Method::PATCH, path, changes).await,
        StatusCode::OK,
    )
    .await
}

pub async fn delete(server: &Server, owner: &Value, path: &str) -> Value {
    data(
        request(server, owner, Method::DELETE, path, Value::Null).await,
        StatusCode::OK,
    )
    .await
}

pub async fn raw(
    server: &Server,
    owner: &Value,
    method: Method,
    path: &str,
    body: String,
) -> Response {
    server
        .client
        .request(method, format!("http://{}{path}", server.address))
        .bearer_auth(owner["access_token"].as_str().unwrap())
        .header("content-type", "application/json")
        .body(body)
        .send()
        .await
        .unwrap()
}
