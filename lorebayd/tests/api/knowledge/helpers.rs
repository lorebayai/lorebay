use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use url::form_urlencoded::Serializer;

use crate::helpers::{Server, assert_problem, create_space, list_page, request};

pub const PATH: &str = "/api/v1/spaces/SPACE-1/knowledge";
pub const CONTENT_LIMIT: usize = 1024 * 1024;
pub const ENTRY_BODY_LIMIT: usize = 6 * CONTENT_LIMIT + 12 * (200 + 500 + 200) + 6 + 1024;

pub async fn setup() -> (Server, Value) {
    let server = Server::executable().await;
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

pub fn resource(path: &str, suffix: &str) -> String {
    path.split_once('?').map_or_else(
        || format!("{path}/{suffix}"),
        |(path, query)| format!("{path}/{suffix}?{query}"),
    )
}

pub fn version_metadata(entry: &Value) -> Value {
    json!({
        "version": entry["version"], "kind": entry["kind"],
        "applied_by": entry["applied_by"], "applied_at": entry["applied_at"],
        "request_key": null,
    })
}

pub async fn roll_back(server: &Server, caller: &Value, path: &str, version: i64) -> Value {
    data(
        request(
            server,
            caller,
            Method::POST,
            &resource(path, "rollback"),
            json!({"version": version, "replaced_version":read(server, caller, path).await["version"]}),
        )
        .await,
        StatusCode::OK,
    )
    .await
}

pub async fn assert_history_target_error(
    server: &Server,
    caller: &Value,
    target: &str,
    status: StatusCode,
    code: &str,
) {
    for (method, suffix) in [
        (Method::GET, "versions"),
        (Method::GET, "versions/1"),
        (Method::POST, "rollback"),
    ] {
        assert_problem(
            request(
                server,
                caller,
                method,
                &resource(target, suffix),
                json!({"version":1,"replaced_version":1}),
            )
            .await,
            status,
            code,
        )
        .await;
    }
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

pub async fn change(server: &Server, owner: &Value, path: &str, mut changes: Value) -> Value {
    changes["version"] = read(server, owner, path).await["version"].clone();
    data(
        request(server, owner, Method::PATCH, path, changes).await,
        StatusCode::OK,
    )
    .await
}

pub async fn delete(server: &Server, owner: &Value, path: &str) -> Value {
    let version = read(server, owner, path).await["version"].as_i64().unwrap();
    let separator = if path.contains('?') { '&' } else { '?' };
    let path = format!("{path}{separator}version={version}");
    data(
        request(server, owner, Method::DELETE, &path, Value::Null).await,
        StatusCode::OK,
    )
    .await
}

pub async fn keyword_pages(
    server: &Server,
    owner: &Value,
    keyword: &str,
    expected: usize,
) -> Vec<Value> {
    let mut rows = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut query = vec![("keyword", keyword), ("page_size", "1")];
        if let Some(cursor) = &cursor {
            query.push(("cursor", cursor));
        }
        let page = list_page(server, owner, PATH, &query).await;
        let items = page["data"].as_array().unwrap();
        assert!(items.len() <= 1);
        rows.extend(items.iter().cloned());
        assert!(rows.len() <= expected);
        let Some(next) = page["meta"]["next_cursor"].as_str() else {
            break;
        };
        assert_eq!(items.len(), 1);
        assert_ne!(cursor.as_deref(), Some(next));
        cursor = Some(next.to_owned());
    }
    assert_eq!(rows.len(), expected);
    rows
}

pub async fn unicode_level(server: &Server, owner: &Value, prefix: &str) {
    let entry = seed(server, owner, &format!("{prefix}/Next/entry")).await;
    let page = list_page(server, owner, PATH, &[("prefix", prefix)]).await;
    assert_eq!(page["data"], json!([{"name":format!("{prefix}/Next/")} ]));
    let next = page["data"][0]["name"].as_str().unwrap();
    let page = list_page(server, owner, PATH, &[("prefix", next)]).await;
    assert_eq!(page["data"][0]["key"], entry["key"]);
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
