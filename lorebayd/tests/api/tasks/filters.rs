use reqwest::{Method, StatusCode};
use sea_orm::sea_query::{Alias, Expr, ExprTrait, Query};
use sea_orm::{ConnectionTrait, Database};
use serde_json::{Value, json};

use crate::helpers::{
    Server, assert_problem, authorize_for, create_space, list_page, list_response, login, request,
};

use super::helpers::{SPACE, TASKS, append, create, participant, setup};

#[tokio::test]
async fn list_tasks_returns_only_matching_tasks_when_filtered_by_person_agent_and_status() {
    let (server, person, agent) = setup().await;
    let shared = authorize_for(&server, &person, "Shared").await;
    let bob = login(&server, "bob").await;
    let bob_shared = authorize_for(&server, &bob, "Shared").await;
    let bob_other = authorize_for(&server, &bob, "Other").await;
    participant(&server, &person, Method::POST, "contributor").await;
    for (tokens, title) in [
        (&agent, "Alice research"),
        (&shared, "Alice shared"),
        (&bob_shared, "Bob shared"),
        (&bob_other, "Bob other"),
        (&shared, "Alice ended"),
    ] {
        create(&server, tokens, SPACE, title).await;
    }
    end_task(&server, 5).await;
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
                    ("status", "in progress"),
                ],
                vec!["TK-2"],
            ),
            (vec![("status", "ended")], vec!["TK-5"]),
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
    participant(&server, &person, Method::POST, "contributor").await;
    create(&server, &other, SPACE, "Historical task").await;
    create(&server, &agent, SPACE, "Alice task").await;
    participant(&server, &person, Method::DELETE, "").await;
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
    participant(&server, &person, Method::POST, "contributor").await;
    create(&server, &agent, SPACE, "Alice current").await;
    let task = create(&server, &second, SPACE, "Alice second").await;
    create(&server, &agent, SPACE, "Alice ended").await;
    create(&server, &other, SPACE, "Bob current").await;
    end_task(&server, 3).await;
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
    append(&server, &agent, &task, "Found and continued").await;
    append(&server, &agent, &task, "Still working").await;
    participant(&server, &person, Method::PATCH, "viewer").await;
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
        ("status", "in progress"),
        ("keyword", "Matching"),
    ];
    let mut query = filters.to_vec();
    query.push(("page_size", "1"));
    let page = list_page(&server, &person, TASKS, &query).await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    for (field, value) in [
        ("person", "bob"),
        ("agent", "Other"),
        ("status", "ended"),
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
        (vec![("status", "unknown")], "status"),
        (vec![("keyword", "")], "keyword"),
        (vec![("keyword", "  ")], "keyword"),
        (vec![("own_open", "true"), ("person", "alice")], "person"),
        (vec![("own_open", "true"), ("status", "ended")], "status"),
        (
            vec![("own_open", "true"), ("status", "in progress")],
            "status",
        ),
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

async fn end_task(server: &Server, number: i64) {
    let path = server.directory.path().join("data/lorebay.sqlite3");
    let database = Database::connect(format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    let statement = Query::update()
        .table(Alias::new("task"))
        .value(Alias::new("status"), Expr::value("ended"))
        .value(Alias::new("ended_at"), Expr::col(Alias::new("started_at")))
        .and_where(Expr::col(Alias::new("number")).eq(number))
        .to_owned();
    database.execute(&statement).await.unwrap();
    database.close().await.unwrap();
}

fn keys(page: &Value) -> Vec<&str> {
    page["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["key"].as_str().unwrap())
        .collect()
}
