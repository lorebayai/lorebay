use std::collections::{HashMap, HashSet};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::time::{sleep, timeout};
use url::Url;

use crate::helpers::{
    PKCE_VERIFIER, REDIRECT_URI as TOKEN_REDIRECT_URI, Server, TEST_PASSWORD, TEST_USERNAME,
    assert_taken,
};

const AGENT_NAME: &str = "Build agent";
const READ_PATH: &str = "/api/v1/authorization-request";
const DECISION_PATH: &str = "/api/v1/authorization-decision";
const REDIRECT_URI: &str = "http://127.0.0.1:46137/callback?tenant=example";
const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const STATE: &str = "state + /?&=%# unchanged";

#[tokio::test]
async fn read_authorization_request_returns_client_and_default_agent_name() {
    let server = Server::start().await;
    let input = registered_request(&server).await;
    let tokens = server.tokens().await;
    for resource in [
        Value::Null,
        json!(format!("http://{}", server.address)),
        input["resource"].clone(),
    ] {
        let mut input = input.clone();
        input["resource"] = resource;
        let response = operation(&server, READ_PATH, input, Some(&tokens)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({
                "data": {"client_name": "Example client", "redirect_host": "127.0.0.1",
                    "default_agent_name": "Example client"}, "meta": {}
            })
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn read_authorization_request_fails_when_caller_is_not_signed_in() {
    let server = Server::start().await;
    let mut input = registered_request(&server).await;
    input["decision"] = json!("approve");
    for path in [READ_PATH, DECISION_PATH] {
        let response = operation(&server, path, input.clone(), None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(!response.headers().contains_key("location"));
        assert_eq!(
            response.json::<Value>().await.unwrap()["code"],
            "auth.unauthorized"
        );
    }
    let tokens = server.tokens().await;
    assert_eq!(
        server
            .log_out(tokens["access_token"].as_str())
            .await
            .status(),
        StatusCode::OK
    );
    for path in [READ_PATH, DECISION_PATH] {
        let response = operation(&server, path, input.clone(), Some(&tokens)).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    server.stop().await;
}

#[tokio::test]
async fn read_authorization_request_fails_when_redirect_uri_is_not_registered() {
    let server = Server::start().await;
    let input = registered_request(&server).await;
    let tokens = server.tokens().await;
    assert_eq!(
        operation(&server, READ_PATH, input.clone(), Some(&tokens))
            .await
            .status(),
        StatusCode::OK
    );
    for redirect in [
        "https://untrusted.invalid/callback",
        "http://127.0.0.1:46138/other?tenant=example",
        "http://127.0.0.1:46138/other/../callback?tenant=example",
        "http://127.1:46138/callback?tenant=example",
        "http://[::1]:46138/callback?tenant=example",
        "https://127.0.0.1:46138/callback?tenant=example",
        "http://127.0.0.1:46137/callback?tenant=example&extra=1",
        "http://127.0.0.1:46137/callback?tenant=%65xample",
    ] {
        let mut input = input.clone();
        input["redirect_uri"] = json!(redirect);
        assert_rejected_by_both(&server, input, &tokens, "redirect_uri").await;
    }
    for (registered, requested) in [
        (
            "http://localhost:46137/callback",
            "http://localhost:46138/callback",
        ),
        (
            "https://127.0.0.1:46137/callback",
            "https://127.0.0.1:46138/callback",
        ),
    ] {
        let response = server
            .json(
                Method::POST,
                "/oauth/register",
                json!({"client_name": "Exact client", "redirect_uris": [registered],
                "token_endpoint_auth_method": "none"}),
                None,
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let client = response.json::<Value>().await.unwrap();
        let input =
            authorization_request(client["client_id"].as_str().unwrap(), requested, &server);
        assert_rejected_by_both(&server, input, &tokens, "redirect_uri").await;
    }
    server.stop().await;
}

#[tokio::test]
async fn read_authorization_request_fails_when_unused_client_was_removed() {
    let server = Server::configured(|config| config.credential_cleanup_interval_seconds = 1).await;
    let unused = server.register_client("Unused").await;
    let revoked = server.authorize_agent("Revoked").await;
    let pending = server.register_client("Pending").await;
    let person = server.tokens().await;
    let code = server
        .approve_code_for(&pending, None, person["access_token"].as_str().unwrap())
        .await;
    let response = server
        .json(
            Method::DELETE,
            "/api/v1/agents/Revoked",
            Value::Null,
            person["access_token"].as_str(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let input = authorization_request(&unused, TOKEN_REDIRECT_URI, &server);
    assert_eq!(
        operation(&server, READ_PATH, input.clone(), Some(&person))
            .await
            .status(),
        StatusCode::OK
    );
    server
        .expire_oauth_credential("oauth_client", "created_at")
        .await;
    timeout(Duration::from_secs(15), async {
        while operation(&server, READ_PATH, input.clone(), Some(&person))
            .await
            .status()
            == StatusCode::OK
        {
            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    assert_rejected_by_both(&server, input, &person, "client_id").await;
    for client in [&revoked.client_id, &pending] {
        let input = authorization_request(client, TOKEN_REDIRECT_URI, &server);
        assert_eq!(
            operation(&server, READ_PATH, input, Some(&person))
                .await
                .status(),
            StatusCode::OK
        );
    }
    successful_tokens(server.exchange_code(&pending, &code).await).await;
    server.stop().await;
}

#[tokio::test]
async fn decide_authorization_returns_redirect_with_code_when_loopback_port_differs() {
    let server = Server::start().await;
    let person = server.tokens().await;
    for (registered, requested) in [
        (
            REDIRECT_URI,
            "http://127.0.0.1:46138/callback?tenant=example",
        ),
        (
            "http://[::1]:46137/callback?tenant=example",
            "http://[::1]:46138/callback?tenant=example",
        ),
        (
            "http://127.0.0.1/callback",
            "http://127.0.0.1:46139/callback",
        ),
        (
            "http://[0:0:0:0:0:0:0:1]:46137/callback",
            "http://[0:0:0:0:0:0:0:1]:46138/callback",
        ),
    ] {
        let response = server
            .json(
                Method::POST,
                "/oauth/register",
                json!({"client_name": requested, "redirect_uris": [registered],
                "token_endpoint_auth_method": "none"}),
                None,
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        let client = response.json::<Value>().await.unwrap();
        let client = client["client_id"].as_str().unwrap();
        let mut input = authorization_request(client, requested, &server);
        assert_eq!(
            operation(&server, READ_PATH, input.clone(), Some(&person))
                .await
                .status(),
            StatusCode::OK
        );
        input["decision"] = json!("approve");
        let query = decide(&server, input, &person).await;
        let mut form = vec![
            ("grant_type", "authorization_code"),
            ("client_id", client),
            ("code", query["code"].as_str()),
            ("redirect_uri", registered),
            ("code_verifier", PKCE_VERIFIER),
        ];
        let response = server.token_request(&form).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.json::<Value>().await.unwrap()["error"],
            "invalid_grant"
        );
        form[3].1 = requested;
        successful_tokens(server.token_request(&form).await).await;
    }
    server.stop().await;
}

#[tokio::test]
async fn read_authorization_request_fails_when_code_challenge_is_missing() {
    let server = Server::start().await;
    let input = registered_request(&server).await;
    let tokens = server.tokens().await;
    for (field, value) in [
        ("code_challenge", Value::Null),
        ("code_challenge", json!("too-short")),
        ("code_challenge_method", json!("plain")),
        ("code_challenge_method", Value::Null),
        ("response_type", json!("token")),
        ("resource", json!("https://untrusted.invalid/mcp")),
        ("resource", json!(format!("http://{}/mcp/", server.address))),
    ] {
        let mut input = input.clone();
        input[field] = value;
        assert_rejected_by_both(&server, input, &tokens, field).await;
    }
    let mut missing = input;
    missing.as_object_mut().unwrap().remove("code_challenge");
    assert_rejected_by_both(&server, missing, &tokens, "code_challenge").await;
    server.stop().await;
}

#[tokio::test]
async fn read_authorization_request_fails_when_client_is_unknown() {
    let server = Server::start().await;
    let mut input = registered_request(&server).await;
    input["client_id"] = json!("unknown-client");
    let tokens = server.tokens().await;
    assert_rejected_by_both(&server, input, &tokens, "client_id").await;
    server.stop().await;
}

#[tokio::test]
async fn decide_authorization_returns_redirect_with_code_when_person_approves() {
    let server = Server::configured(|config| {
        config.public_url = "https://lorebay.test:8443/".to_owned();
    })
    .await;
    let mut input = registered_request(&server).await;
    input["resource"] = json!("https://lorebay.test:8443/mcp");
    input["decision"] = json!("approve");
    input["agent_name"] = json!("Example client (home)");
    let tokens = server.tokens().await;
    let mut codes = HashSet::new();
    for _ in 0..2 {
        let query = decide(&server, input.clone(), &tokens).await;
        assert_eq!(query["tenant"], "example");
        assert_eq!(query["state"], STATE);
        assert!(!query.contains_key("error"));
        assert_eq!(query["code"].len(), 22);
        assert!(codes.insert(query["code"].clone()));
    }
    input["agent_name"] = json!("  ");
    assert_invalid(
        operation(&server, DECISION_PATH, input, Some(&tokens)).await,
        "agent_name",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn decide_authorization_fails_when_agent_name_is_dot_segment() {
    let server = Server::start().await;
    let tokens = server.tokens().await;
    for name in [".", ".."] {
        let client = server.register_client(name).await;
        let mut input = authorization_request(&client, TOKEN_REDIRECT_URI, &server);
        input["decision"] = json!("approve");
        assert_invalid(
            operation(&server, DECISION_PATH, input.clone(), Some(&tokens)).await,
            "agent_name",
        )
        .await;
        input["agent_name"] = json!(name);
        assert_invalid(
            operation(&server, DECISION_PATH, input.clone(), Some(&tokens)).await,
            "agent_name",
        )
        .await;
        input["agent_name"] = json!(format!("Build {name}"));
        let query = decide(&server, input, &tokens).await;
        assert_eq!(
            server.exchange_code(&client, &query["code"]).await.status(),
            StatusCode::OK
        );
    }
    let response = server
        .json(
            Method::GET,
            "/api/v1/agents",
            Value::Null,
            tokens["access_token"].as_str(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.json::<Value>().await.unwrap();
    let names: Vec<_> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|agent| agent["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Build .", "Build .."]);
    server.stop().await;
}

#[tokio::test]
async fn decide_authorization_returns_access_denied_redirect_when_person_declines() {
    let server = Server::configured(|config| {
        config.public_url = "https://lorebay.test:8443/".to_owned();
    })
    .await;
    let mut input = registered_request(&server).await;
    input["resource"] = json!("https://lorebay.test:8443");
    input["decision"] = json!("decline");
    let tokens = server.tokens().await;
    let query = decide(&server, input.clone(), &tokens).await;
    assert_eq!(query["error"], "access_denied");
    assert_eq!(query["state"], STATE);
    assert!(!query.contains_key("code"));
    input.as_object_mut().unwrap().remove("state");
    assert!(!decide(&server, input, &tokens).await.contains_key("state"));
    server.stop().await;
}

#[tokio::test]
async fn decide_authorization_returns_redirect_with_code_when_client_is_cli() {
    let server = Server::start().await;
    let tokens = server.tokens().await;
    for redirect in [
        "http://127.0.0.1:45123/callback",
        "http://localhost:54123/other",
        "http://[::1]:63123/callback",
    ] {
        let mut input = authorization_request("lorebay-cli", redirect, &server);
        let response = operation(&server, READ_PATH, input.clone(), Some(&tokens)).await;
        assert_eq!(response.status(), StatusCode::OK);
        let details = response.json::<Value>().await.unwrap();
        assert_eq!(details["data"]["client_name"], "lorebay CLI");
        assert_eq!(details["data"]["default_agent_name"], Value::Null);
        input["decision"] = json!("approve");
        assert!(decide(&server, input, &tokens).await.contains_key("code"));
    }
    for redirect in [
        "https://untrusted.invalid/callback",
        "http://localhost.evil.invalid/callback",
        "http://192.0.2.1/callback",
        "http://user@localhost/callback",
        "http://localhost/callback#fragment",
        "ftp://localhost/callback",
    ] {
        let input = authorization_request("lorebay-cli", redirect, &server);
        assert_rejected_by_both(&server, input, &tokens, "redirect_uri").await;
    }
    let mut input =
        authorization_request("lorebay-cli", "http://127.0.0.1:45123/callback", &server);
    input["decision"] = json!("approve");
    input["agent_name"] = json!("unwanted agent");
    assert_invalid(
        operation(&server, DECISION_PATH, input, Some(&tokens)).await,
        "agent_name",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn get_authorization_page_returns_page_without_external_scripts() {
    let server = Server::start().await;
    let response = server
        .get("/oauth/authorize?client_id=untrusted%3Cscript%3E")
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["content-type"],
        "text/html; charset=utf-8"
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert_eq!(response.headers()["x-frame-options"], "DENY");
    let policy = response.headers()["content-security-policy"]
        .to_str()
        .unwrap()
        .to_owned();
    let page = response.text().await.unwrap();
    let script = page
        .split_once("<script>")
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    let hash = STANDARD.encode(Sha256::digest(script.as_bytes()));
    assert_eq!(
        policy,
        format!(
            "default-src 'none'; script-src 'sha256-{hash}'; connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'"
        )
    );
    assert_eq!(page.matches("<script").count(), 1);
    for external in [
        "src=",
        "href=",
        "http://",
        "https://",
        "localStorage",
        "sessionStorage",
        "document.cookie",
        "untrusted<script>",
    ] {
        assert!(!page.contains(external), "page contains {external}");
    }
    server.stop().await;
}

#[tokio::test]
async fn decide_authorization_fails_when_agent_name_is_taken() {
    let server = Server::start().await;
    let old = server.authorize_agent(AGENT_NAME).await;
    let client = server.register_client(AGENT_NAME).await;
    let person = server.tokens().await;
    for client in [&client, &old.client_id] {
        let mut input = authorization_request(client, TOKEN_REDIRECT_URI, &server);
        input["decision"] = json!("approve");
        assert_taken(operation(&server, DECISION_PATH, input.clone(), Some(&person)).await).await;
        let mut explicit = input;
        explicit["agent_name"] = json!(AGENT_NAME);
        explicit["replace_existing"] = json!(false);
        assert_taken(operation(&server, DECISION_PATH, explicit, Some(&person)).await).await;
    }
    let mut input = authorization_request(&client, TOKEN_REDIRECT_URI, &server);
    input["decision"] = json!("approve");
    input["agent_name"] = json!("Another agent");
    let code = decide(&server, input, &person).await["code"].clone();
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
        let mut input = authorization_request(&client, TOKEN_REDIRECT_URI, &server);
        input["decision"] = json!("approve");
        input["agent_name"] = json!(name);
        let code = decide(&server, input, &person).await["code"].clone();
        successful_tokens(server.exchange_code(&client, &code).await).await;
    }
    let response = server.log_in("bob", TEST_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bob = response.json::<Value>().await.unwrap()["data"].clone();
    let mut input = authorization_request(&client, TOKEN_REDIRECT_URI, &server);
    input["decision"] = json!("approve");
    input["replace_existing"] = json!(true);
    let code = decide(&server, input, &bob).await["code"].clone();
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
    let mut input = authorization_request(&client, TOKEN_REDIRECT_URI, &server);
    input["decision"] = json!("approve");
    assert_taken(operation(&server, DECISION_PATH, input, Some(&person)).await).await;
    server.stop().await;
}

async fn registered_request(server: &Server) -> Value {
    let response = server
        .json(
            Method::POST,
            "/oauth/register",
            json!({
                "client_name": "Example client", "redirect_uris": [REDIRECT_URI],
                "token_endpoint_auth_method": "none"
            }),
            None,
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let client = response.json::<Value>().await.unwrap();
    authorization_request(client["client_id"].as_str().unwrap(), REDIRECT_URI, server)
}

fn authorization_request(client: &str, redirect: &str, server: &Server) -> Value {
    json!({"client_id": client, "redirect_uri": redirect, "response_type": "code",
        "code_challenge": CHALLENGE, "code_challenge_method": "S256", "state": STATE,
        "resource": format!("http://{}/mcp", server.address)})
}

async fn operation(server: &Server, path: &str, input: Value, tokens: Option<&Value>) -> Response {
    server
        .json(
            Method::POST,
            path,
            input,
            tokens.and_then(|tokens| tokens["access_token"].as_str()),
        )
        .await
}

async fn assert_rejected_by_both(server: &Server, mut input: Value, tokens: &Value, field: &str) {
    assert_invalid(
        operation(server, READ_PATH, input.clone(), Some(tokens)).await,
        field,
    )
    .await;
    for decision in ["approve", "decline"] {
        input["decision"] = json!(decision);
        assert_invalid(
            operation(server, DECISION_PATH, input.clone(), Some(tokens)).await,
            field,
        )
        .await;
    }
}

async fn assert_invalid(response: Response, field: &str) {
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!response.headers().contains_key("location"));
    let problem = response.json::<Value>().await.unwrap();
    assert_eq!(problem["code"], "request.invalid_body");
    assert_eq!(problem["errors"][0]["field"], field);
    assert!(problem.get("redirect_url").is_none());
}

async fn decide(server: &Server, input: Value, tokens: &Value) -> HashMap<String, String> {
    let expected = Url::parse(input["redirect_uri"].as_str().unwrap()).unwrap();
    let response = operation(server, DECISION_PATH, input, Some(tokens)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body = response.json::<Value>().await.unwrap();
    let redirect = Url::parse(body["data"]["redirect_url"].as_str().unwrap()).unwrap();
    assert_eq!(redirect.origin(), expected.origin());
    assert_eq!(redirect.path(), expected.path());
    let query: HashMap<_, _> = redirect
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(query.len(), redirect.query_pairs().count());
    let metadata = server
        .get("/.well-known/oauth-authorization-server")
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(query["iss"], metadata["issuer"].as_str().unwrap());
    query
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
