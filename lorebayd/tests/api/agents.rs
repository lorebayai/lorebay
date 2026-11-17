use std::time::Duration;

use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::time::{sleep, timeout};
use url::Url;

use crate::helpers::{
    AuthorizedAgent, REDIRECT_URI, Server, Settings, TEST_PASSWORD, TEST_TIMEOUT, TEST_USERNAME,
};

#[tokio::test]
async fn list_agents_returns_own_agents_only() {
    let server = two_people().await;
    let alice = server.tokens().await;
    let bob = bob_tokens(&server).await;
    let client = server.register_client("Shared client").await;
    for (name, person) in [
        ("Shared name", &alice),
        ("Alice only", &alice),
        ("Shared name", &bob),
        ("Bob only", &bob),
    ] {
        authorize(&server, &client, name, person).await;
    }
    for (person, expected) in [
        (&alice, json!(["Alice only", "Shared name"])),
        (&bob, json!(["Bob only", "Shared name"])),
    ] {
        let page = list(&server, person, &[]).await;
        assert_eq!(names(&page), expected);
        assert_eq!(page["meta"], json!({}));
        for agent in page["data"].as_array().unwrap() {
            assert_eq!(agent.as_object().unwrap().len(), 3);
            assert_eq!(agent["client_id"], client);
            assert!(agent["created_at"].as_i64().unwrap() > 0);
        }
    }
    assert_problem(
        server
            .json(Method::GET, AGENTS_PATH, Value::Null, None)
            .await,
        StatusCode::UNAUTHORIZED,
        "auth.unauthorized",
    )
    .await;
    assert_problem(
        revoke(&server, "Alice only", None).await,
        StatusCode::UNAUTHORIZED,
        "auth.unauthorized",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn list_agents_returns_next_page_when_cursor_is_given() {
    let server = two_people().await;
    let person = server.tokens().await;
    seed_agents(&server, &person).await;
    let expected: Vec<_> = (0..101).map(|i| format!("Agent-{i:03}")).collect();
    let first = list(&server, &person, &[]).await;
    assert_eq!(names(&first), json!(&expected[..20]));
    let cursor = first["meta"]["next_cursor"].as_str().unwrap();
    assert!(!cursor.contains("Agent-019"));
    let second = list(&server, &person, &[("cursor", cursor)]).await;
    assert_eq!(names(&second), json!(&expected[20..40]));
    let largest = list(&server, &person, &[("page_size", "100")]).await;
    assert_eq!(names(&largest), json!(&expected[..100]));
    let cursor = largest["meta"]["next_cursor"].as_str().unwrap();
    let last = list(&server, &person, &[("cursor", cursor)]).await;
    assert_eq!(names(&last), json!(&expected[100..]));
    assert_eq!(last["meta"], json!({}));
    assert_eq!(walk_pages(&server, &person).await, expected);
    assert_eq!(
        revoke(&server, "Agent-019", person["access_token"].as_str())
            .await
            .status(),
        StatusCode::OK
    );
    let after_deleted = list(
        &server,
        &person,
        &[("cursor", first["meta"]["next_cursor"].as_str().unwrap())],
    )
    .await;
    assert_eq!(names(&after_deleted), json!(&expected[20..40]));
    let bob = bob_tokens(&server).await;
    assert_invalid_page(&server, &bob, &[("cursor", cursor)], "cursor").await;
    for size in ["0", "101", "18446744073709551615"] {
        assert_invalid_page(&server, &person, &[("page_size", size)], "page_size").await;
    }
    assert_invalid_page(&server, &person, &[("page_size", "text")], "query").await;
    for cursor in ["", "not-a-cursor", "YWdlbnRzLXYx"] {
        assert_invalid_page(&server, &person, &[("cursor", cursor)], "cursor").await;
    }
    server.stop().await;
}

#[tokio::test]
async fn revoke_agent_ends_agent_authorization() {
    let server = Server::start().await;
    let person = server.tokens().await;
    let name = "Build agent / home? #+%";
    let agent = server.authorize_agent(name).await;
    let access = agent.tokens["access_token"].as_str();
    assert_eq!(agent.tokens["expires_in"], 900);
    assert_person_required(list_response(&server, &agent.tokens, &[]).await).await;
    let response = revoke(&server, name, person["access_token"].as_str()).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data": null, "meta": {}})
    );
    assert_problem(
        server
            .json(Method::GET, AGENTS_PATH, Value::Null, access)
            .await,
        StatusCode::UNAUTHORIZED,
        "auth.unauthorized",
    )
    .await;
    assert_invalid_refresh(&server, &agent).await;
    assert_eq!(names(&list(&server, &person, &[]).await), json!([]));
    assert_problem(
        revoke(&server, name, person["access_token"].as_str()).await,
        StatusCode::NOT_FOUND,
        "http.not_found",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn revoke_agent_fails_when_agent_belongs_to_another_person() {
    let server = two_people().await;
    let alice = server.tokens().await;
    let bob = bob_tokens(&server).await;
    let agent = server.authorize_agent("Alice only").await;
    assert_problem(
        revoke(&server, "Alice only", bob["access_token"].as_str()).await,
        StatusCode::NOT_FOUND,
        "http.not_found",
    )
    .await;
    assert_eq!(
        names(&list(&server, &alice, &[]).await),
        json!(["Alice only"])
    );
    assert_eq!(names(&list(&server, &bob, &[]).await), json!([]));
    let response = server
        .refresh_oauth(
            &agent.client_id,
            agent.tokens["refresh_token"].as_str().unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let tokens = response.json::<Value>().await.unwrap();
    assert_ne!(tokens["refresh_token"], agent.tokens["refresh_token"]);
    assert_person_required(list_response(&server, &tokens, &[]).await).await;
    server.stop().await;
}

#[tokio::test]
async fn list_agents_fails_when_caller_is_agent() {
    let server = Server::start().await;
    let person = server.tokens().await;
    let agent = server.authorize_agent("Build agent").await;
    assert_person_required(list_response(&server, &agent.tokens, &[]).await).await;
    assert_person_required(
        revoke(
            &server,
            "Build agent",
            agent.tokens["access_token"].as_str(),
        )
        .await,
    )
    .await;
    let request = json!({"client_id": agent.client_id, "redirect_uri": REDIRECT_URI,
        "response_type": "code", "code_challenge": PKCE_CHALLENGE,
        "code_challenge_method": "S256", "decision": "approve",
        "agent_name": "Another agent"});
    for path in [
        "/api/v1/authorization-request",
        "/api/v1/authorization-decision",
    ] {
        assert_person_required(
            server
                .json(
                    Method::POST,
                    path,
                    request.clone(),
                    agent.tokens["access_token"].as_str(),
                )
                .await,
        )
        .await;
    }
    assert_person_required(server.log_out(agent.tokens["access_token"].as_str()).await).await;
    assert_eq!(
        names(&list(&server, &person, &[]).await),
        json!(["Build agent"])
    );
    assert_person_required(list_response(&server, &agent.tokens, &[]).await).await;
    server.stop().await;
}

#[tokio::test]
async fn list_agents_omits_agent_when_authorization_has_expired() {
    let server = Server::configured(|config| config.credential_cleanup_interval_seconds = 1).await;
    let person = server.tokens().await;
    let agent = server.authorize_agent("Build agent").await;
    assert_eq!(
        names(&list(&server, &person, &[]).await),
        json!(["Build agent"])
    );
    server
        .expire_oauth_credential("agent_authorization", "refresh_expires_at")
        .await;
    timeout(TEST_TIMEOUT, async {
        while !list(&server, &person, &[]).await["data"]
            .as_array()
            .unwrap()
            .is_empty()
        {
            sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert_problem(
        list_response(&server, &agent.tokens, &[]).await,
        StatusCode::UNAUTHORIZED,
        "auth.unauthorized",
    )
    .await;
    assert_invalid_refresh(&server, &agent).await;
    let replacement = authorize(&server, &agent.client_id, "Build agent", &person).await;
    assert_ne!(
        replacement.tokens["access_token"],
        agent.tokens["access_token"]
    );
    assert_eq!(
        names(&list(&server, &person, &[]).await),
        json!(["Build agent"])
    );
    assert_person_required(list_response(&server, &replacement.tokens, &[]).await).await;
    server.stop().await;
}

#[tokio::test]
async fn call_web_api_fails_when_agent_authorization_has_expired() {
    let server = Server::start().await;
    let person = server.tokens().await;
    let agent = server.authorize_agent("Build agent").await;
    assert_person_required(list_response(&server, &agent.tokens, &[]).await).await;
    server
        .expire_oauth_credential("agent_authorization", "refresh_expires_at")
        .await;
    assert_problem(
        list_response(&server, &agent.tokens, &[]).await,
        StatusCode::UNAUTHORIZED,
        "auth.unauthorized",
    )
    .await;
    assert_eq!(
        names(&list(&server, &person, &[]).await),
        json!(["Build agent"])
    );
    server.stop().await;
}

#[tokio::test]
async fn call_web_api_fails_when_agent_access_token_is_invalid() {
    let server =
        Server::start_with_settings(Settings::builder().access_seconds(2_u64).build()).await;
    let agent = server.authorize_agent("Build agent").await;
    assert_person_required(list_response(&server, &agent.tokens, &[]).await).await;
    let access = agent.tokens["access_token"].as_str().unwrap();
    let (claims, signature) = access.rsplit_once('.').unwrap();
    let first = if signature.starts_with('A') { "B" } else { "A" };
    let forged = format!("{claims}.{first}{}", &signature[1..]);
    assert_problem(
        server
            .json(Method::GET, AGENTS_PATH, Value::Null, Some(&forged))
            .await,
        StatusCode::UNAUTHORIZED,
        "auth.unauthorized",
    )
    .await;
    sleep(Duration::from_secs(3)).await;
    assert_problem(
        list_response(&server, &agent.tokens, &[]).await,
        StatusCode::UNAUTHORIZED,
        "auth.unauthorized",
    )
    .await;
    let response = server
        .refresh_oauth(
            &agent.client_id,
            agent.tokens["refresh_token"].as_str().unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let refreshed = response.json::<Value>().await.unwrap();
    assert_person_required(list_response(&server, &refreshed, &[]).await).await;
    server.stop().await;
}

#[tokio::test]
async fn call_web_api_fails_when_agent_account_was_removed() {
    let server = Server::start().await;
    let agent = server.authorize_agent("Build agent").await;
    assert_person_required(list_response(&server, &agent.tokens, &[]).await).await;
    let directory = server.stop().await;
    let server = Server::restart_with_users(directory, &[]).await;
    assert_problem(
        list_response(&server, &agent.tokens, &[]).await,
        StatusCode::UNAUTHORIZED,
        "auth.unauthorized",
    )
    .await;
    assert_invalid_refresh(&server, &agent).await;
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    let person = server.tokens().await;
    assert_eq!(names(&list(&server, &person, &[]).await), json!([]));
    assert_problem(
        list_response(&server, &agent.tokens, &[]).await,
        StatusCode::UNAUTHORIZED,
        "auth.unauthorized",
    )
    .await;
    server.stop().await;
}

const AGENTS_PATH: &str = "/api/v1/agents";
const BOB_USERNAME: &str = "bob";
const PKCE_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

async fn two_people() -> Server {
    Server::restart_with_users(
        TempDir::new().unwrap(),
        &[
            (TEST_USERNAME, TEST_PASSWORD),
            (BOB_USERNAME, TEST_PASSWORD),
        ],
    )
    .await
}

async fn bob_tokens(server: &Server) -> Value {
    let response = server.log_in(BOB_USERNAME, TEST_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["data"].clone()
}

async fn authorize(server: &Server, client: &str, name: &str, person: &Value) -> AuthorizedAgent {
    let code = server
        .approve_code_for(client, Some(name), person["access_token"].as_str().unwrap())
        .await;
    let response = server.exchange_code(client, &code).await;
    assert_eq!(response.status(), StatusCode::OK);
    AuthorizedAgent::builder()
        .client_id(client)
        .tokens(response.json::<Value>().await.unwrap())
        .build()
}

async fn seed_agents(server: &Server, person: &Value) {
    let client = server.register_client("Pagination client").await;
    for i in (0..101).rev() {
        authorize(server, &client, &format!("Agent-{i:03}"), person).await;
    }
}

async fn walk_pages(server: &Server, person: &Value) -> Vec<String> {
    let mut result = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut query = vec![("page_size", "17")];
        if let Some(cursor) = &cursor {
            query.push(("cursor", cursor));
        }
        let page = list(server, person, &query).await;
        result.extend(
            page["data"]
                .as_array()
                .unwrap()
                .iter()
                .map(|agent| agent["name"].as_str().unwrap().to_owned()),
        );
        cursor = page["meta"]["next_cursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            return result;
        }
    }
}

async fn list_response(server: &Server, tokens: &Value, query: &[(&str, &str)]) -> Response {
    let mut url = Url::parse(&format!("http://{}{AGENTS_PATH}", server.address)).unwrap();
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    server
        .client
        .get(url)
        .bearer_auth(tokens["access_token"].as_str().unwrap())
        .send()
        .await
        .unwrap()
}

async fn list(server: &Server, tokens: &Value, query: &[(&str, &str)]) -> Value {
    let response = list_response(server, tokens, query).await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

async fn revoke(server: &Server, name: &str, token: Option<&str>) -> Response {
    let mut url = Url::parse(&format!("http://{}{AGENTS_PATH}", server.address)).unwrap();
    url.path_segments_mut().unwrap().push(name);
    let mut request = server.client.delete(url);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    request.send().await.unwrap()
}

fn names(page: &Value) -> Value {
    json!(
        page["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|agent| agent["name"].clone())
            .collect::<Vec<_>>()
    )
}

async fn assert_problem(response: Response, status: StatusCode, code: &str) -> Value {
    assert_eq!(response.status(), status);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    if status == StatusCode::UNAUTHORIZED {
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
    }
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(body["status"], status.as_u16());
    assert_eq!(body["code"], code);
    body
}

async fn assert_person_required(response: Response) {
    let body = assert_problem(response, StatusCode::FORBIDDEN, "auth.person_required").await;
    assert_eq!(
        body,
        json!({"type": "about:blank", "title": "A login session is required",
        "status": 403, "code": "auth.person_required", "errors": []})
    );
}

async fn assert_invalid_page(server: &Server, tokens: &Value, query: &[(&str, &str)], field: &str) {
    let body = assert_problem(
        list_response(server, tokens, query).await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
    assert_eq!(body["errors"][0]["field"], field);
}

async fn assert_invalid_refresh(server: &Server, agent: &AuthorizedAgent) {
    let response = server
        .refresh_oauth(
            &agent.client_id,
            agent.tokens["refresh_token"].as_str().unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"],
        "invalid_grant"
    );
}
