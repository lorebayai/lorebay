use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::helpers::{Server, TEST_PASSWORD, assert_problem, list_response};
use super::participants::helpers::{PARTICIPANTS, list, login, setup, walk};

#[tokio::test]
async fn list_users_returns_every_username() {
    let mut names: Vec<_> = (0..23).map(|index| format!("user-{index:02}")).collect();
    names.push("alice".into());
    let users: Vec<_> = names
        .iter()
        .map(|name| (name.as_str(), TEST_PASSWORD))
        .collect();
    let server = Server::restart_with_users(TempDir::new().unwrap(), &users).await;
    let owner = server.tokens().await;
    names.sort();
    let expected: Vec<_> = names
        .iter()
        .map(|username| json!({"username":username}))
        .collect();
    assert_eq!(walk(&server, &owner, USERS_PATH, "7").await, expected);
    assert_eq!(
        list(&server, &owner, USERS_PATH)
            .await
            .as_array()
            .unwrap()
            .len(),
        20
    );
    let response = list_response(USERS_PATH, &server, &owner, &[("page_size", "100")]).await;
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data":expected,"meta":{}})
    );
    assert_user_cursors(&server, &owner).await;
    let directory = server.stop().await;
    let server = Server::restart_with_users(directory, &[("alice", TEST_PASSWORD)]).await;
    let owner = server.tokens().await;
    assert_eq!(
        walk(&server, &owner, USERS_PATH, "1").await,
        vec![json!({"username":"alice"})]
    );
    server.stop().await;
}

#[tokio::test]
async fn list_users_fails_when_caller_is_agent() {
    let (server, owner) = setup().await;
    let agent = server.authorize_agent("Owner agent").await;
    assert_eq!(
        list(&server, &owner, USERS_PATH)
            .await
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_problem(
        list_response(USERS_PATH, &server, &agent.tokens, &[]).await,
        StatusCode::FORBIDDEN,
        "auth.person_required",
    )
    .await;
    for key in ["SPACE-1", "SPACE-999"] {
        for method in [Method::POST, Method::PATCH, Method::DELETE] {
            let response = server
                .json(
                    method,
                    &format!("/api/v1/spaces/{key}/participants/bob"),
                    json!({"role":"viewer"}),
                    Some(agent.tokens["access_token"].as_str().unwrap()),
                )
                .await;
            assert_problem(response, StatusCode::FORBIDDEN, "auth.person_required").await;
        }
    }
    assert_eq!(
        list(&server, &owner, PARTICIPANTS)
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
    server.stop().await;
}

const USERS_PATH: &str = "/api/v1/users";

async fn assert_user_cursors(server: &Server, owner: &Value) {
    let response = list_response(USERS_PATH, server, owner, &[("page_size", "1")]).await;
    let page = response.json::<Value>().await.unwrap();
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    let other = login(server, "user-00").await;
    assert_problem(
        list_response(USERS_PATH, server, &other, &[("cursor", cursor)]).await,
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
            list_response(USERS_PATH, server, owner, &query).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
    assert_problem(
        server.get(USERS_PATH).await,
        StatusCode::UNAUTHORIZED,
        "auth.unauthorized",
    )
    .await;
}
