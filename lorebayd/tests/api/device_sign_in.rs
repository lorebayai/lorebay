use std::time::Duration;

use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use tokio::time::sleep;
use url::Url;

use crate::helpers::{Server, create_space, request};

const CLI_CLIENT: &str = "lorebay-cli";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const READ_PATH: &str = "/api/v1/device-sign-in-request";
const DECISION_PATH: &str = "/api/v1/device-sign-in-decision";

#[tokio::test]
async fn request_device_code_returns_user_code_and_verification_address() {
    let server =
        Server::configured(|config| config.public_url = "https://lorebay.test/".to_owned()).await;
    let request = device_request(&server, CLI_CLIENT).await;
    let body = request.json::<Value>().await.unwrap();
    assert_eq!(
        body["verification_uri"],
        "https://lorebay.test/oauth/device/verify"
    );
    let address = Url::parse(body["verification_uri_complete"].as_str().unwrap()).unwrap();
    assert_eq!(
        address.origin().ascii_serialization(),
        "https://lorebay.test"
    );
    assert_eq!(
        address.query_pairs().next().unwrap().1,
        body["user_code"].as_str().unwrap()
    );
    assert_eq!(body["expires_in"], 600);
    assert_eq!(body["interval"], 5);
    let code = body["user_code"].as_str().unwrap();
    assert_eq!(code.len(), 9);
    assert_eq!(&code[4..5], "-");
    assert!(
        code.bytes()
            .filter(|byte| *byte != b'-')
            .all(|byte| b"BCDFGHJKLMNPQRSTVWXZ".contains(&byte))
    );
    assert!(body["device_code"].as_str().unwrap().len() >= 43);
    let person = server.tokens().await;
    let response = read(&server, &code.replace('-', "").to_lowercase(), &person).await;
    assert_eq!(response.status(), StatusCode::OK);
    let details = response.json::<Value>().await.unwrap();
    assert_eq!(details["data"]["user_code"], code);
    assert!(details["data"]["created_at"].as_i64().unwrap() > 0);
    server.stop().await;
}

#[tokio::test]
async fn request_device_code_fails_when_client_is_not_cli() {
    let server = Server::start().await;
    let client = server.register_client("Device client").await;
    assert_error(
        device_request(&server, &client).await,
        "unauthorized_client",
    )
    .await;
    let codes = codes(&server).await;
    assert_error(
        exchange(&server, &client, &codes).await,
        "unauthorized_client",
    )
    .await;
    let person = server.tokens().await;
    assert_decision(decide(&server, &codes, &person, "approve").await).await;
    let response = exchange(&server, CLI_CLIENT, &codes).await;
    assert_eq!(response.status(), StatusCode::OK);
    server.stop().await;
}

#[tokio::test]
async fn request_device_code_fails_when_rate_limit_is_exceeded() {
    let server = Server::configured(|config| {
        config.device_rate_limit = 1;
        config.device_rate_window_seconds = 1;
    })
    .await;
    assert_eq!(
        device_request(&server, CLI_CLIENT).await.status(),
        StatusCode::OK
    );
    let response = device_request(&server, CLI_CLIENT).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"],
        "slow_down"
    );
    sleep(Duration::from_millis(1100)).await;
    assert_eq!(
        device_request(&server, CLI_CLIENT).await.status(),
        StatusCode::OK
    );
    server.stop().await;
}

#[tokio::test]
async fn exchange_device_code_returns_login_session_when_person_approved() {
    let server = Server::configured(|config| config.device_poll_interval_seconds = 1).await;
    let codes = codes(&server).await;
    assert_error(
        exchange(&server, CLI_CLIENT, &codes).await,
        "authorization_pending",
    )
    .await;
    let person = server.tokens().await;
    assert_eq!(
        read(&server, codes["user_code"].as_str().unwrap(), &person)
            .await
            .status(),
        StatusCode::OK
    );
    assert_decision(decide(&server, &codes, &person, "approve").await).await;
    assert_eq!(
        decide(&server, &codes, &person, "decline").await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        server.log_out(Some(token(&person))).await.status(),
        StatusCode::OK
    );
    sleep(Duration::from_millis(1100)).await;
    let (first, second) = tokio::join!(
        exchange(&server, CLI_CLIENT, &codes),
        exchange(&server, CLI_CLIENT, &codes)
    );
    let (accepted, refused) = if first.status().is_success() {
        (first, second)
    } else {
        (second, first)
    };
    assert_error(refused, "invalid_grant").await;
    assert_cli_session(&server, accepted, &person).await;
    assert_error(exchange(&server, CLI_CLIENT, &codes).await, "invalid_grant").await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_device_code_fails_when_person_declined() {
    let server = Server::start().await;
    let codes = codes(&server).await;
    let person = server.tokens().await;
    assert_decision(decide(&server, &codes, &person, "decline").await).await;
    assert_eq!(
        decide(&server, &codes, &person, "approve").await.status(),
        StatusCode::BAD_REQUEST
    );
    assert_error(exchange(&server, CLI_CLIENT, &codes).await, "access_denied").await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_device_code_fails_when_request_is_expired() {
    let server = Server::start().await;
    let codes = codes(&server).await;
    server
        .expire_oauth_credential("device_sign_in_request", "expires_at")
        .await;
    assert_error(exchange(&server, CLI_CLIENT, &codes).await, "expired_token").await;
    let person = server.tokens().await;
    assert_eq!(
        read(&server, codes["user_code"].as_str().unwrap(), &person)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        decide(&server, &codes, &person, "approve").await.status(),
        StatusCode::BAD_REQUEST
    );
    server.stop().await;
}

#[tokio::test]
async fn exchange_device_code_fails_with_slow_down_when_polled_too_fast() {
    let server = Server::configured(|config| config.device_poll_interval_seconds = 1).await;
    let codes = codes(&server).await;
    assert_error(
        exchange(&server, CLI_CLIENT, &codes).await,
        "authorization_pending",
    )
    .await;
    assert_error(exchange(&server, CLI_CLIENT, &codes).await, "slow_down").await;
    sleep(Duration::from_millis(1100)).await;
    assert_error(exchange(&server, CLI_CLIENT, &codes).await, "slow_down").await;
    server.stop().await;
}

#[tokio::test]
async fn read_device_request_fails_when_user_code_is_unknown() {
    let server = Server::configured(|config| {
        config.device_verification_rate_limit = 2;
        config.device_verification_rate_window_seconds = 1;
    })
    .await;
    let person = server.tokens().await;
    let response = read(&server, "AAAA-AAAA", &person).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.json::<Value>().await.unwrap()["errors"][0]["field"],
        "user_code"
    );
    let unknown = json!({"user_code":"AAAA-AAAA"});
    assert_eq!(
        decide(&server, &unknown, &person, "approve").await.status(),
        StatusCode::BAD_REQUEST
    );
    let codes = codes(&server).await;
    let response = read(&server, codes["user_code"].as_str().unwrap(), &person).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(response.headers().contains_key("retry-after"));
    assert_eq!(
        decide(&server, &codes, &person, "approve").await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    sleep(Duration::from_millis(1100)).await;
    assert_eq!(
        read(&server, codes["user_code"].as_str().unwrap(), &person)
            .await
            .status(),
        StatusCode::OK
    );
    server.stop().await;
}

#[tokio::test]
async fn read_device_request_fails_when_caller_is_agent() {
    let server = Server::start().await;
    let agent = server.authorize_agent("Device agent").await;
    let codes = codes(&server).await;
    let response = read(&server, codes["user_code"].as_str().unwrap(), &agent.tokens).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.json::<Value>().await.unwrap()["code"],
        "auth.person_required"
    );
    let response = decide(&server, &codes, &agent.tokens, "approve").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response.json::<Value>().await.unwrap()["code"],
        "auth.person_required"
    );
    server.stop().await;
}

#[tokio::test]
async fn get_device_page_returns_page_without_external_scripts() {
    let server = Server::start().await;
    let response = server.get("/oauth/device/verify?user_code=BCDF-GHJK").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    let policy = response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(policy.starts_with("default-src 'none'; script-src 'sha256-"));
    assert!(policy.ends_with(
        "connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
    ));
    let page = response.text().await.unwrap();
    assert!(page.contains("Confirm only a sign-in you started"));
    assert!(page.contains("Requested at:"));
    assert!(page.contains("<script>"));
    assert!(!page.contains("src="));
    assert!(!page.contains("localStorage"));
    assert!(!page.contains("sessionStorage"));
    server.stop().await;
}

async fn device_request(server: &Server, client: &str) -> Response {
    server
        .client
        .post(format!("http://{}/oauth/device", server.address))
        .header("host", "untrusted.example")
        .form(&[("client_id", client)])
        .send()
        .await
        .unwrap()
}

async fn codes(server: &Server) -> Value {
    let response = device_request(server, CLI_CLIENT).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    response.json().await.unwrap()
}

async fn exchange(server: &Server, client: &str, codes: &Value) -> Response {
    server
        .token_request(&[
            ("grant_type", DEVICE_GRANT),
            ("client_id", client),
            ("device_code", codes["device_code"].as_str().unwrap()),
        ])
        .await
}

async fn read(server: &Server, user_code: &str, tokens: &Value) -> Response {
    request(
        server,
        tokens,
        Method::POST,
        READ_PATH,
        json!({"user_code":user_code}),
    )
    .await
}

async fn decide(server: &Server, codes: &Value, tokens: &Value, decision: &str) -> Response {
    request(
        server,
        tokens,
        Method::POST,
        DECISION_PATH,
        json!({"user_code":codes["user_code"],"decision":decision}),
    )
    .await
}

async fn assert_error(response: Response, expected: &str) {
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.json::<Value>().await.unwrap()["error"], expected);
}

fn token(tokens: &Value) -> &str {
    tokens["access_token"].as_str().unwrap()
}

async fn assert_cli_session(server: &Server, accepted: Response, person: &Value) {
    assert_eq!(accepted.status(), StatusCode::OK);
    assert_eq!(accepted.headers()["cache-control"], "no-store");
    let tokens = accepted.json::<Value>().await.unwrap();
    server.assert_token_issuer(&tokens).await;
    assert_eq!(tokens["token_type"], "Bearer");
    create_space(server, &tokens, "Device space", "CLI sign-in").await;
    let listed = request(server, &tokens, Method::GET, "/api/v1/spaces", Value::Null).await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(
        request(server, person, Method::GET, "/api/v1/spaces", Value::Null)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let replacement = server.tokens().await;
    let same_space = request(
        server,
        &replacement,
        Method::GET,
        "/api/v1/spaces/SPACE-1",
        Value::Null,
    )
    .await;
    assert_eq!(same_space.status(), StatusCode::OK);
    assert_eq!(
        same_space.json::<Value>().await.unwrap()["data"]["role"],
        "owner"
    );
    assert_session_refresh(server, &tokens).await;
}

async fn assert_session_refresh(server: &Server, tokens: &Value) {
    let refreshed = server
        .refresh_oauth(CLI_CLIENT, tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(refreshed.status(), StatusCode::OK);
    let refreshed = refreshed.json::<Value>().await.unwrap();
    assert_eq!(
        server.log_out(Some(token(&refreshed))).await.status(),
        StatusCode::OK
    );
    assert_error(
        server
            .refresh_oauth(CLI_CLIENT, refreshed["refresh_token"].as_str().unwrap())
            .await,
        "invalid_grant",
    )
    .await;
}

async fn assert_decision(response: Response) {
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data": null, "meta": {}})
    );
}
