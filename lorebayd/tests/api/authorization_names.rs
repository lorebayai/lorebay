use std::time::Duration;

use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::time::{sleep, timeout};
use url::Url;

use crate::helpers::{PKCE_VERIFIER, REDIRECT_URI, Server, TEST_PASSWORD, TEST_USERNAME};

const AGENT_NAME: &str = "Build agent";
const DECISION_PATH: &str = "/api/v1/authorization-decision";
const PKCE_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

#[tokio::test]
async fn decide_authorization_fails_when_agent_name_is_taken() {
    let server = Server::start().await;
    let old = server.authorize_agent(AGENT_NAME).await;
    let client = server.register_client(AGENT_NAME).await;
    let person = server.tokens().await;
    for client in [&client, &old.client_id] {
        let input = decision(client);
        assert_taken(submit(&server, input.clone(), &person).await).await;
        let mut explicit = input;
        explicit["agent_name"] = json!(AGENT_NAME);
        explicit["replace_existing"] = json!(false);
        assert_taken(submit(&server, explicit, &person).await).await;
    }
    let mut input = decision(&client);
    input["agent_name"] = json!("Another agent");
    let code = approved_code(submit(&server, input, &person).await).await;
    successful_tokens(server.exchange_code(&client, &code).await).await;
    successful_tokens(
        server
            .refresh_oauth(
                &old.client_id,
                old.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn decide_authorization_compares_agent_names_exactly_for_each_person() {
    let server = Server::restart_with_users(
        TempDir::new().unwrap(),
        &[(TEST_USERNAME, TEST_PASSWORD), ("bob", TEST_PASSWORD)],
    )
    .await;
    let old = server.authorize_agent(AGENT_NAME).await;
    let client = server.register_client(AGENT_NAME).await;
    let person = server.tokens().await;
    for name in ["build agent", "Build agent "] {
        let mut input = decision(&client);
        input["agent_name"] = json!(name);
        let code = approved_code(submit(&server, input, &person).await).await;
        successful_tokens(server.exchange_code(&client, &code).await).await;
    }
    let response = server.log_in("bob", TEST_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bob = response.json::<Value>().await.unwrap()["data"].clone();
    let mut input = decision(&client);
    input["replace_existing"] = json!(true);
    let code = approved_code(submit(&server, input, &bob).await).await;
    let tokens = successful_tokens(server.exchange_code(&client, &code).await).await;
    successful_tokens(
        server
            .refresh_oauth(&client, tokens["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
    successful_tokens(
        server
            .refresh_oauth(
                &old.client_id,
                old.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
    )
    .await;
    assert_taken(submit(&server, decision(&client), &person).await).await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_replaces_old_authorization_when_person_chose_to_replace() {
    let server = Server::start().await;
    let old = server.authorize_agent(AGENT_NAME).await;
    let client = server.register_client(AGENT_NAME).await;
    let person = server.tokens().await;
    let code = replacement_code(&server, &client, &person).await;
    let old_tokens = successful_tokens(
        server
            .refresh_oauth(
                &old.client_id,
                old.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
    )
    .await;
    let new = successful_tokens(server.exchange_code(&client, &code).await).await;
    assert_ne!(new["access_token"], old_tokens["access_token"]);
    assert_invalid_grant(
        server
            .refresh_oauth(
                &old.client_id,
                old_tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
    )
    .await;
    successful_tokens(
        server
            .refresh_oauth(&client, new["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
    assert_invalid_grant(server.exchange_code(&client, &code).await).await;
    assert_taken(submit(&server, decision(&client), &person).await).await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_preserves_old_authorization_when_replacement_verifier_is_wrong() {
    let server = Server::start().await;
    let old = server.authorize_agent(AGENT_NAME).await;
    let client = server.register_client(AGENT_NAME).await;
    let person = server.tokens().await;
    let code = replacement_code(&server, &client, &person).await;
    assert_invalid_grant(
        server
            .token_request(&[
                ("grant_type", "authorization_code"),
                ("client_id", &client),
                ("code", &code),
                ("redirect_uri", REDIRECT_URI),
                ("code_verifier", &PKCE_VERIFIER.repeat(2)),
            ])
            .await,
    )
    .await;
    successful_tokens(
        server
            .refresh_oauth(
                &old.client_id,
                old.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
    )
    .await;
    successful_tokens(server.exchange_code(&client, &code).await).await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_fails_when_replacement_target_was_already_replaced() {
    let server = Server::start().await;
    server.authorize_agent(AGENT_NAME).await;
    let client = server.register_client(AGENT_NAME).await;
    let person = server.tokens().await;
    let first = replacement_code(&server, &client, &person).await;
    let second = replacement_code(&server, &client, &person).await;
    let tokens = successful_tokens(server.exchange_code(&client, &first).await).await;
    assert_invalid_grant(server.exchange_code(&client, &second).await).await;
    successful_tokens(
        server
            .refresh_oauth(&client, tokens["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_succeeds_when_replaced_authorization_was_removed() {
    let server = Server::configured(|config| {
        config.credential_cleanup_interval_seconds = 1;
    })
    .await;
    let old = server.authorize_agent(AGENT_NAME).await;
    let person = server.tokens().await;
    let client = server.register_client(AGENT_NAME).await;
    let code = replacement_code(&server, &client, &person).await;
    server
        .expire_oauth_credential("agent_authorization", "refresh_expires_at")
        .await;
    timeout(Duration::from_secs(5), async {
        loop {
            let response = submit(&server, decision(&client), &person).await;
            if response.status() == StatusCode::OK {
                approved_code(response).await;
                break;
            }
            assert_taken(response).await;
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    let new = successful_tokens(server.exchange_code(&client, &code).await).await;
    assert_invalid_grant(
        server
            .refresh_oauth(
                &old.client_id,
                old.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
    )
    .await;
    successful_tokens(
        server
            .refresh_oauth(&client, new["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
    assert_taken(submit(&server, decision(&client), &person).await).await;
    server.stop().await;
}

fn decision(client_id: &str) -> Value {
    json!({"client_id": client_id, "redirect_uri": REDIRECT_URI,
        "response_type": "code", "code_challenge": PKCE_CHALLENGE,
        "code_challenge_method": "S256", "decision": "approve"})
}

async fn submit(server: &Server, input: Value, person: &Value) -> Response {
    server
        .json(
            Method::POST,
            DECISION_PATH,
            input,
            person["access_token"].as_str(),
        )
        .await
}

async fn approved_code(response: Response) -> String {
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.unwrap();
    Url::parse(body["data"]["redirect_url"].as_str().unwrap())
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "code")
        .unwrap()
        .1
        .into_owned()
}

async fn replacement_code(server: &Server, client: &str, person: &Value) -> String {
    let mut input = decision(client);
    input["replace_existing"] = json!(true);
    approved_code(submit(server, input, person).await).await
}

async fn assert_taken(response: Response) {
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!response.headers().contains_key("location"));
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"type": "about:blank", "title": "Bad Request", "status": 400,
            "code": "request.invalid_body", "errors": [{"field": "agent_name",
                "code": "taken", "message": "You already have an agent with this name."}]})
    );
}

async fn successful_tokens(response: Response) -> Value {
    assert_eq!(response.status(), StatusCode::OK);
    let tokens = response.json::<Value>().await.unwrap();
    assert_eq!(tokens["token_type"], "Bearer");
    assert!(
        tokens["refresh_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty())
    );
    assert!(
        tokens["access_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty())
    );
    tokens
}

async fn assert_invalid_grant(response: Response) {
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"],
        "invalid_grant"
    );
}
