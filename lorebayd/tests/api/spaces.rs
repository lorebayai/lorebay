use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{
    Server, assert_problem, create_space, list_page, list_response, login, request, two_people,
    walk,
};

#[tokio::test]
async fn create_space_makes_creator_owner() {
    let server = Server::start().await;
    let person = server.tokens().await;
    for number in 1..=2 {
        let space = create_space(&server, &person, "Research", "Shared research").await;
        let expected = json!({"key": format!("SPACE-{number}"), "name": "Research",
            "summary": "Shared research", "role": "owner"});
        assert_eq!(space, expected);
        assert_eq!(
            read(&server, &person, space["key"].as_str().unwrap()).await,
            expected
        );
    }
    let (third, fourth) = tokio::join!(
        create_space(&server, &person, "Concurrent A", "Shared research"),
        create_space(&server, &person, "Concurrent B", "Shared research"),
    );
    let mut keys = [
        third["key"].as_str().unwrap(),
        fourth["key"].as_str().unwrap(),
    ];
    keys.sort_unstable();
    assert_eq!(keys, ["SPACE-3", "SPACE-4"]);
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    let person = server.tokens().await;
    assert_eq!(
        create_space(&server, &person, "After restart", "Shared research").await["key"],
        "SPACE-5"
    );
    assert_eq!(read(&server, &person, "SPACE-1").await["role"], "owner");
    server.stop().await;
}

#[tokio::test]
async fn create_space_fails_when_caller_is_agent() {
    let server = Server::start().await;
    let person = server.tokens().await;
    let space = create_space(&server, &person, "Research", "Shared research").await;
    let agent = server.authorize_agent("Research agent").await;
    let refusal = assert_problem(
        request(
            &server,
            &agent.tokens,
            Method::POST,
            SPACES_PATH,
            json!({"name": "Forbidden", "summary": "Forbidden"}),
        )
        .await,
        StatusCode::FORBIDDEN,
        "auth.person_required",
    )
    .await;
    for key in ["SPACE-1", "SPACE-999", "invalid"] {
        assert_eq!(
            assert_problem(
                request(
                    &server,
                    &agent.tokens,
                    Method::PATCH,
                    &format!("{SPACES_PATH}/{key}"),
                    json!({"name": "Forbidden"})
                )
                .await,
                StatusCode::FORBIDDEN,
                "auth.person_required",
            )
            .await,
            refusal
        );
    }
    assert_eq!(read(&server, &person, "SPACE-1").await, space);
    assert_eq!(
        list_page(&server, &person, SPACES_PATH, &[]).await["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    server.stop().await;
}

#[tokio::test]
async fn create_space_fails_when_name_is_empty() {
    let server = Server::start().await;
    let person = server.tokens().await;
    for body in [
        json!({"name": "", "summary": "Summary"}),
        json!({"name": "   ", "summary": "Summary"}),
        json!({"name": "Name", "summary": ""}),
        json!({"summary": "Summary"}),
        json!({"name": "Name"}),
        json!({"name": "a".repeat(257), "summary": "Summary"}),
        json!({"name": "Name", "summary": "a".repeat(4097)}),
    ] {
        assert_problem(
            request(&server, &person, Method::POST, SPACES_PATH, body).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    assert_eq!(
        list_page(&server, &person, SPACES_PATH, &[]).await["data"],
        json!([])
    );
    let response = request(
        &server,
        &person,
        Method::POST,
        SPACES_PATH,
        json!({"name": "a".repeat(256), "summary": "a".repeat(4096)}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let original = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(original["key"], "SPACE-1");
    for body in [
        json!({}),
        json!({"name": ""}),
        json!({"summary": ""}),
        json!({"name": null}),
        json!({"summary": "a".repeat(4097)}),
        json!({"name": "a".repeat(257)}),
    ] {
        assert_problem(
            request(
                &server,
                &person,
                Method::PATCH,
                "/api/v1/spaces/SPACE-1",
                body,
            )
            .await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
        assert_eq!(read(&server, &person, "SPACE-1").await, original);
    }
    server.stop().await;
}

#[tokio::test]
async fn list_spaces_returns_only_spaces_caller_is_in() {
    let server = two_people().await;
    let alice = server.tokens().await;
    let bob = login(&server, "bob").await;
    assert_eq!(
        list_page(&server, &alice, SPACES_PATH, &[]).await,
        json!({"data": [], "meta": {}})
    );
    create_space(&server, &bob, "Bob only", "Shared research").await;
    let mut expected = Vec::new();
    for number in 2..=23 {
        expected.push(
            create_space(
                &server,
                &alice,
                &format!("Space {number}"),
                "Shared research",
            )
            .await,
        );
    }
    let agent = server.authorize_agent("Research agent").await;
    assert_eq!(walk(&server, &alice, SPACES_PATH, "7", 22).await, expected);
    let expected_agent: Vec<_> = expected
        .into_iter()
        .map(|mut space| {
            space["role"] = json!("contributor");
            space
        })
        .collect();
    assert_eq!(
        walk(&server, &agent.tokens, SPACES_PATH, "7", 22).await,
        expected_agent
    );
    assert_eq!(
        read(&server, &agent.tokens, "SPACE-2").await,
        expected_agent[0]
    );
    let bob_spaces = list_page(&server, &bob, SPACES_PATH, &[]).await;
    assert_eq!(bob_spaces["data"].as_array().unwrap().len(), 1);
    assert_eq!(bob_spaces["data"][0]["key"], "SPACE-1");
    assert_eq!(
        list_page(&server, &alice, SPACES_PATH, &[]).await["data"]
            .as_array()
            .unwrap()
            .len(),
        20
    );
    assert_eq!(
        list_page(&server, &alice, SPACES_PATH, &[("page_size", "100")]).await["data"]
            .as_array()
            .unwrap()
            .len(),
        22
    );
    assert_invalid_pagination(&server, &alice, &bob).await;
    server.stop().await;
}

#[tokio::test]
async fn read_space_fails_when_caller_is_not_in_space() {
    let server = two_people().await;
    let alice = server.tokens().await;
    let bob = login(&server, "bob").await;
    let original = create_space(&server, &bob, "Bob only", "Shared research").await;
    let agent = server.authorize_agent("Research agent").await;
    let mut refusal = None;
    for tokens in [&alice, &agent.tokens] {
        for key in ["SPACE-1", "SPACE-999", "SPACE-0", "SPACE-01", "invalid"] {
            let body = assert_problem(
                request(
                    &server,
                    tokens,
                    Method::GET,
                    &format!("{SPACES_PATH}/{key}"),
                    Value::Null,
                )
                .await,
                StatusCode::NOT_FOUND,
                "space.not_found",
            )
            .await;
            if let Some(expected) = &refusal {
                assert_eq!(&body, expected);
            } else {
                refusal = Some(body);
            }
            let (status, code) = if tokens == &alice {
                (StatusCode::NOT_FOUND, "space.not_found")
            } else {
                (StatusCode::FORBIDDEN, "auth.person_required")
            };
            let changed = assert_problem(
                request(
                    &server,
                    tokens,
                    Method::PATCH,
                    &format!("{SPACES_PATH}/{key}"),
                    json!({"name": "Forbidden"}),
                )
                .await,
                status,
                code,
            )
            .await;
            if tokens == &alice {
                assert_eq!(&changed, refusal.as_ref().unwrap());
            }
        }
    }
    assert_eq!(read(&server, &bob, "SPACE-1").await, original);
    assert_eq!(
        list_page(&server, &alice, SPACES_PATH, &[]).await["data"],
        json!([])
    );
    assert_eq!(
        list_page(&server, &agent.tokens, SPACES_PATH, &[]).await["data"],
        json!([])
    );
    server.stop().await;
}

#[tokio::test]
async fn change_space_updates_name_and_summary_when_caller_is_owner() {
    let server = Server::start().await;
    let person = server.tokens().await;
    create_space(&server, &person, "Research", "Shared research").await;
    let mut expected = json!({"key": "SPACE-1", "name": "Research",
        "summary": "Shared research", "role": "owner"});
    for changes in [
        json!({"name": "New name"}),
        json!({"summary": "New summary"}),
        json!({"name": "Final name", "summary": "Final summary"}),
    ] {
        for (field, value) in changes.as_object().unwrap() {
            expected[field] = value.clone();
        }
        let response = request(
            &server,
            &person,
            Method::PATCH,
            "/api/v1/spaces/SPACE-1",
            changes,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({"data": expected, "meta": {}})
        );
        assert_eq!(read(&server, &person, "SPACE-1").await, expected);
    }
    assert_eq!(
        list_page(&server, &person, SPACES_PATH, &[]).await["data"],
        json!([expected])
    );
    server.stop().await;
}

const SPACES_PATH: &str = "/api/v1/spaces";

async fn read(server: &Server, tokens: &Value, key: &str) -> Value {
    let response = request(
        server,
        tokens,
        Method::GET,
        &format!("{SPACES_PATH}/{key}"),
        Value::Null,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["data"].clone()
}

async fn assert_invalid_pagination(server: &Server, alice: &Value, bob: &Value) {
    let page = list_page(server, alice, SPACES_PATH, &[("page_size", "1")]).await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    assert_problem(
        list_response(SPACES_PATH, server, bob, &[("cursor", cursor)]).await,
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
            list_response(SPACES_PATH, server, alice, &query).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
}
