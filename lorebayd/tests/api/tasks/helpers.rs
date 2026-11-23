use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{
    Server, assert_problem, authorize_for, create_space, list_page, list_response, request,
    two_people,
};

pub const SPACE: &str = "SPACE-1";
pub const TASKS: &str = "/api/v1/spaces/SPACE-1/tasks";

pub async fn setup() -> (Server, Value, Value) {
    let server = two_people().await;
    let person = server.tokens().await;
    create_space(&server, &person, "Team", "Shared work").await;
    let agent = authorize_for(&server, &person, "Research agent").await;
    (server, person, agent)
}

pub async fn create(server: &Server, tokens: &Value, space: &str, title: &str) -> Value {
    let response = request(
        server,
        tokens,
        Method::POST,
        &format!("/api/v1/spaces/{space}/tasks"),
        json!({"title":title}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(body["meta"], json!({}));
    body["data"].clone()
}

pub async fn checkpoint_request(
    server: &Server,
    tokens: &Value,
    space: &str,
    id: &str,
    content: &str,
) -> Response {
    request(
        server,
        tokens,
        Method::POST,
        &format!("/api/v1/spaces/{space}/tasks/{id}/checkpoints"),
        json!({"content":content}),
    )
    .await
}

pub async fn append(server: &Server, tokens: &Value, task: &Value, content: &str) -> Value {
    let response = checkpoint_request(
        server,
        tokens,
        SPACE,
        task["task_id"].as_str().unwrap(),
        content,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json::<Value>().await.unwrap()["data"].clone()
}

pub async fn read(server: &Server, tokens: &Value, key: &str) -> Value {
    list_page(server, tokens, &format!("{TASKS}/{key}"), &[]).await["data"].clone()
}

pub async fn scratchpad(server: &Server, tokens: &Value, key: &str) -> Value {
    list_page(
        server,
        tokens,
        &format!("{TASKS}/{key}/scratchpad"),
        &[("page_size", "100")],
    )
    .await["data"]
        .clone()
}

pub async fn space_member(server: &Server, owner: &Value, method: Method, role: &str) {
    let response = request(
        server,
        owner,
        method.clone(),
        "/api/v1/spaces/SPACE-1/members/bob",
        json!({"role":role}),
    )
    .await;
    let status = match method {
        Method::POST => StatusCode::CREATED,
        _ => StatusCode::OK,
    };
    assert_eq!(response.status(), status);
}

pub async fn read_checkpoint_pages(server: &Server, tokens: &Value, key: &str, expected: &[Value]) {
    let path = format!("{TASKS}/{key}");
    let mut rows = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut query = vec![("include_scratchpad", "true"), ("page_size", "2")];
        if let Some(cursor) = &cursor {
            query.push(("cursor", cursor));
        }
        let page = list_page(server, tokens, &path, &query).await;
        assert_eq!(page["data"]["key"], key);
        let checkpoints = &page["data"]["scratchpad"];
        let items = checkpoints["data"].as_array().unwrap();
        assert!(items.len() <= 2);
        rows.extend(items.iter().cloned());
        assert!(rows.len() <= expected.len());
        match checkpoints["meta"]["next_cursor"].as_str() {
            Some(next) => {
                assert_eq!(items.len(), 2);
                assert_ne!(cursor.as_deref(), Some(next));
                cursor = Some(next.to_owned());
            }
            None => break,
        }
    }
    assert_eq!(rows, expected);
}

pub async fn assert_checkpoint_cursors(
    server: &Server,
    person: &Value,
    bob: &Value,
    expected: &[Value],
) {
    let path = format!("{TASKS}/TK-1");
    let page = list_page(
        server,
        person,
        &path,
        &[("include_scratchpad", "true"), ("page_size", "2")],
    )
    .await;
    let cursor = page["data"]["scratchpad"]["meta"]["next_cursor"]
        .as_str()
        .unwrap();
    let tail = list_page(
        server,
        person,
        &format!("{path}/scratchpad"),
        &[("cursor", cursor)],
    )
    .await;
    assert_eq!(tail["data"], json!(&expected[2..]));
    assert_problem(
        list_response(
            &path,
            server,
            bob,
            &[("include_scratchpad", "true"), ("cursor", cursor)],
        )
        .await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
    for query in [
        [("page_size", "0")],
        [("page_size", "101")],
        [("cursor", "invalid")],
    ] {
        assert_problem(
            list_response(&format!("{path}/scratchpad"), server, person, &query).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
    let agent = authorize_for(server, person, "Paging agent").await;
    let other = create(server, &agent, SPACE, "Other task").await;
    let other_path = format!("{TASKS}/{}/scratchpad", other["key"].as_str().unwrap());
    assert_problem(
        list_response(&other_path, server, person, &[("cursor", cursor)]).await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
}

pub async fn assert_task_cursors(server: &Server, person: &Value, bob: &Value) {
    let page = list_page(server, person, TASKS, &[("page_size", "2")]).await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    assert_problem(
        list_response(TASKS, server, bob, &[("cursor", cursor)]).await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
    create_space(server, person, "Other", "Other work").await;
    assert_problem(
        list_response(
            "/api/v1/spaces/SPACE-2/tasks",
            server,
            person,
            &[("cursor", cursor)],
        )
        .await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
    for query in [
        [("page_size", "0")],
        [("page_size", "101")],
        [("cursor", "invalid")],
    ] {
        assert_problem(
            list_response(TASKS, server, person, &query).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
}
