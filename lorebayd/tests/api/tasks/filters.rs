use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{
    Server, assert_problem, authorize_for, create_space, list_page, list_response, login, request,
};
use crate::mcp::tools::call as call_tool;

use super::helpers::{SPACE, TASKS, append, create, read, setup, space_member};

#[tokio::test]
async fn list_tasks_returns_only_matching_tasks_when_filtered_by_person_agent_and_status() {
    let (server, person, agent) = setup().await;
    let shared = authorize_for(&server, &person, "Shared").await;
    let bob = login(&server, "bob").await;
    let bob_shared = authorize_for(&server, &bob, "Shared").await;
    let bob_other = authorize_for(&server, &bob, "Other").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    for (tokens, title) in [
        (&agent, "Alice research"),
        (&shared, "Alice shared"),
        (&bob_shared, "Bob shared"),
        (&bob_other, "Bob other"),
        (&shared, "Alice ended"),
    ] {
        create(&server, tokens, SPACE, title).await;
    }
    end_task(&server, &person, 5).await;
    create_space(&server, &person, "Other space", "Excluded").await;
    create(&server, &shared, "SPACE-2", "Outside list").await;
    for tokens in [&person, &agent] {
        for (query, expected) in [
            (vec![("person", "alice")], vec!["TK-5", "TK-2", "TK-1"]),
            (vec![("agent", "Shared")], vec!["TK-5", "TK-3", "TK-2"]),
            (
                vec![("person", "alice"), ("agent", "Shared")],
                vec!["TK-5", "TK-2"],
            ),
            (
                vec![
                    ("person", "alice"),
                    ("agent", "Shared"),
                    ("state", "working"),
                ],
                vec!["TK-2"],
            ),
            (vec![("state", "canceled")], vec!["TK-5"]),
            (vec![("person", "missing")], vec![]),
            (vec![("agent", "missing")], vec![]),
        ] {
            assert_keys(&server, tokens, &query, &expected).await;
        }
    }
    let page = list_page(
        &server,
        &person,
        TASKS,
        &[("agent", "Shared"), ("page_size", "1")],
    )
    .await;
    assert_eq!(keys(&page), ["TK-5"]);
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    assert_keys(
        &server,
        &person,
        &[("agent", "Shared"), ("cursor", cursor)],
        &["TK-3", "TK-2"],
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn list_tasks_returns_persons_tasks_when_person_was_removed_from_space() {
    let (server, person, agent) = setup().await;
    let bob = login(&server, "bob").await;
    let other = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    create(&server, &other, SPACE, "Historical task").await;
    create(&server, &agent, SPACE, "Alice task").await;
    space_member(&server, &person, Method::DELETE, "").await;
    assert_keys(&server, &agent, &[("person", "bob")], &["TK-1"]).await;
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    let person = server.tokens().await;
    assert_keys(&server, &person, &[("person", "bob")], &["TK-1"]).await;
    let page = list_page(&server, &person, TASKS, &[("person", "bob")]).await;
    assert_eq!(page["data"][0]["person"], "bob");
    assert_eq!(page["data"][0]["agent_name"], "Bob agent");
    server.stop().await;
}

#[tokio::test]
async fn list_tasks_returns_own_party_tasks_not_ended_when_asked() {
    let (server, person, agent) = setup().await;
    let second = authorize_for(&server, &person, "Second").await;
    let bob = login(&server, "bob").await;
    let other = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    create(&server, &agent, SPACE, "Alice current").await;
    let task = create(&server, &second, SPACE, "Alice second").await;
    create(&server, &agent, SPACE, "Alice ended").await;
    create(&server, &other, SPACE, "Bob current").await;
    end_task(&server, &person, 3).await;
    for tokens in [&person, &agent, &second] {
        assert_keys(&server, tokens, &[("own_open", "true")], &["TK-2", "TK-1"]).await;
        assert_keys(
            &server,
            tokens,
            &[
                ("own_open", "true"),
                ("agent", "Second"),
                ("keyword", "SECOND"),
            ],
            &["TK-2"],
        )
        .await;
    }
    for tokens in [&bob, &other] {
        assert_keys(&server, tokens, &[("own_open", "true")], &["TK-4"]).await;
    }
    call_tool(
        &server,
        &agent,
        "claim_task",
        json!({"task_id":task["task_id"]}),
    )
    .await;
    append(&server, &agent, &task, "Found and continued").await;
    append(&server, &agent, &task, "Still working").await;
    space_member(&server, &person, Method::PATCH, "viewer").await;
    assert_keys(&server, &other, &[("own_open", "true")], &["TK-4"]).await;
    assert_keys(
        &server,
        &agent,
        &[("own_open", "false")],
        &["TK-2", "TK-4", "TK-3", "TK-1"],
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn list_tasks_matches_keyword_in_title_literally() {
    let (server, person, agent) = setup().await;
    for title in [
        "Plan a_b 50% c\\d \u{4e2d}\u{6587} \u{00c9}",
        "Plan axb 500 cd",
        "Words in another order",
    ] {
        create(&server, &agent, SPACE, title).await;
    }
    for keyword in [
        "a_b",
        "50%",
        "c\\d",
        "\u{4e2d}\u{6587}",
        "PLAN A_B",
        "a_b 50%",
        "\u{00c9}",
    ] {
        assert_keys(&server, &agent, &[("keyword", keyword)], &["TK-1"]).await;
    }
    for keyword in ["Plan 50%", "another Words", "\u{00e9}", "' OR 1=1 --"] {
        assert_keys(&server, &person, &[("keyword", keyword)], &[]).await;
    }
    let maximum = "\u{4e2d}".repeat(200);
    create(&server, &agent, SPACE, &maximum).await;
    assert_keys(&server, &person, &[("keyword", &maximum)], &["TK-4"]).await;
    let space = create(&server, &agent, SPACE, "a b").await;
    let underscore = create(&server, &agent, SPACE, "a_b").await;
    assert_ne!(space["key"], underscore["key"]);
    assert_keys(&server, &person, &[("keyword", "a_b")], &["TK-6", "TK-1"]).await;
    server.stop().await;
}

#[tokio::test]
async fn list_tasks_matches_keyword_in_result() {
    let (server, person, agent) = setup().await;
    create(&server, &agent, SPACE, "Searchable title").await;
    let result_task = create(&server, &agent, SPACE, "Findings").await;
    let decoy = create(&server, &agent, SPACE, "Other work").await;
    change(
        &server,
        &agent,
        &result_task,
        json!({"result":"# Searchable result\na_b 50% c\\d \u{4e2d}\u{6587} \u{00c9}"}),
    )
    .await;
    change(&server, &agent, &decoy, json!({"result":"axb 500 cd"})).await;
    for tokens in [&person, &agent] {
        assert_keys(
            &server,
            tokens,
            &[("keyword", "SEARCHABLE")],
            &["TK-2", "TK-1"],
        )
        .await;
        for keyword in [
            "a_b",
            "50%",
            "c\\d",
            "\u{4e2d}\u{6587}",
            "A_B",
            "a_b 50%",
            "\u{00c9}",
        ] {
            assert_keys(&server, tokens, &[("keyword", keyword)], &["TK-2"]).await;
        }
        for keyword in ["a_b c\\d", "\u{00e9}"] {
            assert_keys(&server, tokens, &[("keyword", keyword)], &[]).await;
        }
    }
    assert_keys(
        &server,
        &agent,
        &[
            ("person", "alice"),
            ("agent", "Research agent"),
            ("state", "working"),
            ("keyword", "50%"),
        ],
        &["TK-2"],
    )
    .await;
    replace_search_result(&server, &person, &agent, &result_task).await;
    server.stop().await;
}

#[tokio::test]
async fn list_tasks_filters_revoked_agent_tasks_by_copied_name() {
    let (server, person, _) = setup().await;
    let first = authorize_for(&server, &person, "Shared").await;
    create(&server, &first, SPACE, "Before revocation").await;
    let response = request(
        &server,
        &person,
        Method::DELETE,
        "/api/v1/agents/Shared",
        Value::Null,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_keys(&server, &person, &[("agent", "Shared")], &["TK-1"]).await;
    let second = authorize_for(&server, &person, "Shared").await;
    create(&server, &second, SPACE, "After revocation").await;
    assert_keys(
        &server,
        &second,
        &[("person", "alice"), ("agent", "Shared")],
        &["TK-2", "TK-1"],
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn list_tasks_fails_when_cursor_came_from_other_filters() {
    let (server, person, agent) = setup().await;
    for _ in 0..3 {
        create(&server, &agent, SPACE, "Matching task").await;
    }
    let filters = [
        ("person", "alice"),
        ("agent", "Research agent"),
        ("state", "working"),
        ("keyword", "Matching"),
    ];
    let mut query = filters.to_vec();
    query.push(("page_size", "1"));
    let page = list_page(&server, &person, TASKS, &query).await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    for (field, value) in [
        ("person", "bob"),
        ("agent", "Other"),
        ("state", "canceled"),
        ("keyword", "task"),
    ] {
        for replacement in [None, Some(value)] {
            let mut changed: Vec<_> = filters
                .iter()
                .copied()
                .filter(|(name, _)| *name != field)
                .collect();
            if let Some(value) = replacement {
                changed.push((field, value));
            }
            changed.push(("cursor", cursor));
            assert_query_error(&server, &person, &changed, "cursor").await;
        }
    }
    query.pop();
    query.reverse();
    query.push(("cursor", cursor));
    assert_keys(&server, &person, &query, &["TK-2", "TK-1"]).await;
    assert_own_open_cursors(&server, &person).await;
    server.stop().await;
}

#[tokio::test]
async fn list_tasks_omits_deleted_tasks_when_reusing_filtered_cursor() {
    let (server, person, agent) = setup().await;
    let mut tasks = Vec::new();
    for _ in 0..3 {
        tasks.push(create(&server, &agent, SPACE, "Matching task").await);
    }
    let filters = [
        ("person", "alice"),
        ("agent", "Research agent"),
        ("state", "working"),
        ("keyword", "Matching"),
    ];
    let mut query = filters.to_vec();
    query.push(("page_size", "1"));
    let page = list_page(&server, &person, TASKS, &query).await;
    assert_eq!(keys(&page), ["TK-3"]);
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    for task in [&tasks[2], &tasks[1]] {
        remove(&server, &person, task).await;
    }
    assert_keys(&server, &person, &filters, &["TK-1"]).await;
    query.pop();
    query.push(("cursor", cursor));
    assert_keys(&server, &person, &query, &["TK-1"]).await;
    assert_keys(
        &server,
        &agent,
        &[
            ("own_open", "true"),
            ("agent", "Research agent"),
            ("keyword", "Matching"),
        ],
        &["TK-1"],
    )
    .await;
    remove(&server, &person, &tasks[0]).await;
    assert_keys(&server, &person, &query, &[]).await;
    assert_keys(&server, &agent, &[("own_open", "true")], &[]).await;
    server.stop().await;
}

async fn assert_own_open_cursors(server: &Server, person: &Value) {
    let page = list_page(
        server,
        person,
        TASKS,
        &[("own_open", "true"), ("page_size", "1")],
    )
    .await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    assert_query_error(server, person, &[("cursor", cursor)], "cursor").await;
    assert_keys(
        server,
        person,
        &[("own_open", "true"), ("cursor", cursor)],
        &["TK-2", "TK-1"],
    )
    .await;
    let page = list_page(server, person, TASKS, &[("page_size", "1")]).await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    assert_query_error(
        server,
        person,
        &[("own_open", "true"), ("cursor", cursor)],
        "cursor",
    )
    .await;
}

#[tokio::test]
async fn list_tasks_fails_when_status_is_unknown() {
    let (server, person, agent) = setup().await;
    create(&server, &agent, SPACE, "Unchanged").await;
    for (query, field) in [
        (vec![("state", "unknown")], "state"),
        (vec![("keyword", "")], "keyword"),
        (vec![("keyword", "  ")], "keyword"),
        (vec![("own_open", "true"), ("person", "alice")], "person"),
        (vec![("own_open", "true"), ("state", "canceled")], "state"),
        (vec![("own_open", "true"), ("state", "working")], "state"),
    ] {
        assert_query_error(&server, &person, &query, field).await;
    }
    for keyword in ["x".repeat(201), "\u{4e2d}".repeat(201)] {
        assert_query_error(&server, &agent, &[("keyword", &keyword)], "keyword").await;
    }
    assert_keys(&server, &person, &[], &["TK-1"]).await;
    server.stop().await;
}

async fn assert_keys(server: &Server, tokens: &Value, query: &[(&str, &str)], expected: &[&str]) {
    let page = list_page(server, tokens, TASKS, query).await;
    assert_eq!(keys(&page), expected, "query: {query:?}");
    assert_eq!(page["meta"], json!({}));
}

async fn assert_query_error(server: &Server, tokens: &Value, query: &[(&str, &str)], field: &str) {
    let error = assert_problem(
        list_response(TASKS, server, tokens, query).await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
    assert_eq!(error["errors"][0]["field"], field);
}

async fn change(server: &Server, tokens: &Value, task: &Value, changes: Value) -> Value {
    let response = request(
        server,
        tokens,
        Method::PATCH,
        &format!("{TASKS}/{}", task["task_id"].as_str().unwrap()),
        changes,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(body["meta"], json!({}));
    let changed = body["data"].clone();
    assert_eq!(
        read(server, tokens, task["key"].as_str().unwrap()).await,
        changed
    );
    changed
}

async fn end_task(server: &Server, tokens: &Value, number: i64) {
    let task = read(server, tokens, &format!("TK-{number}")).await;
    let ended = change(
        server,
        tokens,
        &task,
        json!({"state":"canceled", "state_reason":"Fixture complete"}),
    )
    .await;
    assert_eq!(ended["state"], "canceled");
    assert!(ended["ended_at"].is_i64());
}

async fn remove(server: &Server, tokens: &Value, task: &Value) {
    let response = request(
        server,
        tokens,
        Method::DELETE,
        &format!("{TASKS}/{}", task["task_id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data":null,"meta":{}})
    );
}

fn keys(page: &Value) -> Vec<&str> {
    page["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["key"].as_str().unwrap())
        .collect()
}

async fn replace_search_result(server: &Server, person: &Value, agent: &Value, task: &Value) {
    change(
        server,
        agent,
        task,
        json!({"result":"Replacement", "result_version":1}),
    )
    .await;
    assert_keys(server, person, &[("keyword", "a_b")], &[]).await;
    assert_keys(server, person, &[("keyword", "replacement")], &["TK-2"]).await;
    change(
        server,
        agent,
        task,
        json!({"result":"", "result_version":2}),
    )
    .await;
    assert_keys(server, person, &[("keyword", "replacement")], &[]).await;
    assert_keys(server, person, &[("keyword", "searchable")], &["TK-1"]).await;
}
