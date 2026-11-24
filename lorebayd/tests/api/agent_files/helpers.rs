use reqwest::{Method, Response, StatusCode};
use sea_orm::sea_query::{Alias, Expr, ExprTrait, Query};
use sea_orm::{ConnectionTrait, Database};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use url::Url;

use crate::helpers::{Server, TEST_TIMEOUT, authorize_for, create_space, request};
use crate::tasks::helpers::{SPACE, create};

pub async fn prepare(server: &Server) -> (Value, Value, Value) {
    let person = server.tokens().await;
    create_space(server, &person, "Team", "Shared work").await;
    let agent = authorize_for(server, &person, "Upload agent").await;
    let task = create(server, &agent, SPACE, "File work").await;
    (person, agent, task)
}

pub async fn set_status(server: &Server, tokens: &Value, task: &Value, status: &str) -> Value {
    let task_id = task["task_id"].as_str().unwrap();
    let response = request(
        server,
        tokens,
        Method::PATCH,
        &format!("/api/v1/spaces/{SPACE}/tasks/{task_id}"),
        json!({"state":status}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let task = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(task["state"], status);
    task
}

pub async fn register(server: &Server, agent: &Value, task: &Value) -> Value {
    let response = registration(
        server,
        agent,
        SPACE,
        task["task_id"].as_str().unwrap(),
        json!({"file_name":"report.txt", "summary":"The report"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json::<Value>().await.unwrap()["data"].clone()
}

pub async fn registration(
    server: &Server,
    agent: &Value,
    space: &str,
    task: &str,
    body: Value,
) -> Response {
    request(
        server,
        agent,
        Method::POST,
        &format!("/api/v1/spaces/{space}/tasks/{task}/files"),
        body,
    )
    .await
}

pub async fn complete(server: &Server, agent: &Value, task: &Value, key: &str) -> Response {
    completion(server, agent, SPACE, task["task_id"].as_str().unwrap(), key).await
}

pub async fn completion(
    server: &Server,
    agent: &Value,
    space: &str,
    task: &str,
    key: &str,
) -> Response {
    request(
        server,
        agent,
        Method::POST,
        &format!("/api/v1/spaces/{space}/tasks/{task}/files/{key}/complete"),
        Value::Null,
    )
    .await
}

pub fn file_url(server: &Server, url: &str) -> Url {
    let mut url = Url::parse(url).unwrap();
    url.set_port(Some(server.address.port())).unwrap();
    url
}

pub async fn upload(server: &Server, registered: &Value, bytes: &'static str) -> Response {
    let mut request = server
        .client
        .put(file_url(server, registered["upload_url"].as_str().unwrap()))
        .body(bytes);
    for (name, value) in registered["headers"].as_object().unwrap() {
        request = request.header(name, value.as_str().unwrap());
    }
    request
        .send()
        .await
        .map_err(|_| "upload request failed")
        .unwrap()
}

pub async fn upload_ok(server: &Server, registered: &Value, bytes: &'static str) {
    let response = upload(server, registered, bytes).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(response.bytes().await.unwrap().is_empty());
}

pub async fn raw_upload(server: &Server, url: &str, extra: &str) -> TcpStream {
    let url = Url::parse(url).unwrap();
    let mut stream = TcpStream::connect(server.address).await.unwrap();
    let path = format!("{}?{}", url.path(), url.query().unwrap());
    let headers = format!(
        "PUT {path} HTTP/1.1\r\nHost: {}\r\nIf-None-Match: *\r\nExpect: 100-continue\r\n{extra}\r\n",
        server.address
    );
    stream.write_all(headers.as_bytes()).await.unwrap();
    stream
}

pub async fn read_headers(stream: &mut TcpStream) -> String {
    timeout(TEST_TIMEOUT, async {
        let mut bytes = Vec::new();
        while !bytes.ends_with(b"\r\n\r\n") {
            let byte = stream.read_u8().await.unwrap();
            bytes.push(byte);
        }
        String::from_utf8(bytes).unwrap()
    })
    .await
    .unwrap()
}

pub async fn held_upload(server: &Server, registered: &Value) -> TcpStream {
    let mut stream = raw_upload(
        server,
        registered["upload_url"].as_str().unwrap(),
        "Content-Length: 4\r\n",
    )
    .await;
    assert!(
        read_headers(&mut stream)
            .await
            .starts_with("HTTP/1.1 100 Continue")
    );
    stream
}

pub async fn age_registration(server: &Server, key: &str) {
    let database = Database::connect(format!(
        "sqlite://{}",
        server
            .directory
            .path()
            .join("data/lorebay.sqlite3")
            .display()
    ))
    .await
    .unwrap();
    let number = key.strip_prefix("FL-").unwrap().parse::<i64>().unwrap();
    let statement = Query::update()
        .table(Alias::new("space_agent_file"))
        .value(Alias::new("created_at"), Expr::value(0_i64))
        .and_where(Expr::col(Alias::new("number")).eq(number))
        .to_owned();
    database.execute(&statement).await.unwrap();
    database.close().await.unwrap();
}
