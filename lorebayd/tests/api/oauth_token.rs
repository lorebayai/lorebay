use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use tokio::time::{sleep, timeout};
use url::Url;

use crate::helpers::{PKCE_VERIFIER, REDIRECT_URI, Server, Settings, assert_taken};

const AGENT_NAME: &str = "Build agent";
const DECISION_PATH: &str = "/api/v1/authorization-decision";
const PKCE_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
const CLI_CLIENT: &str = "lorebay-cli";
const RENEWAL_WAIT: Duration = Duration::from_millis(1800);

#[tokio::test]
async fn exchange_code_returns_access_and_refresh_tokens() {
    let server = Server::start().await;
    let client = server.register_client("Example client").await;
    let code = server.approve_code(&client, Some("Build agent")).await;
    let tokens = assert_tokens(server.exchange_code(&client, &code).await).await;
    let claims = claims(&tokens);
    assert_eq!(claims["kind"], "agent_authorization");
    assert_eq!(claims["sub"], "alice");
    assert_eq!(claims["sid"].as_str().unwrap().len(), 22);
    assert_eq!(
        claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
        900
    );
    let refreshed = assert_tokens(
        server
            .refresh_oauth(&client, tokens["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
    assert_eq!(claims["sid"], self::claims(&refreshed)["sid"]);
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_creates_login_session_when_client_is_cli() {
    let server = Server::start().await;
    let code = server.approve_code(CLI_CLIENT, None).await;
    let tokens = assert_tokens(server.exchange_code(CLI_CLIENT, &code).await).await;
    assert_eq!(claims(&tokens)["kind"], "login_session");
    assert_eq!(claims(&tokens)["sub"], "alice");
    assert_person(&server, &tokens).await;
    assert_eq!(
        server
            .log_out(tokens["access_token"].as_str())
            .await
            .status(),
        StatusCode::OK
    );
    assert_oauth_error(
        server
            .refresh_oauth(CLI_CLIENT, tokens["refresh_token"].as_str().unwrap())
            .await,
        "invalid_grant",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_fails_when_code_verifier_does_not_match() {
    let server = Server::start().await;
    let client = server.register_client("Example client").await;
    let code = server.approve_code(&client, None).await;
    let mut form = exchange_form(&client, &code);
    form[4].1 = "wrong-verifier-that-is-still-valid-and-long-enough";
    assert_oauth_error(server.token_request(&form).await, "invalid_grant").await;
    assert_tokens(server.exchange_code(&client, &code).await).await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_fails_when_code_was_already_exchanged() {
    let server = Server::start().await;
    let client = server.register_client("Example client").await;
    let code = server.approve_code(&client, None).await;
    let (first, second) = tokio::join!(
        server.exchange_code(&client, &code),
        server.exchange_code(&client, &code),
    );
    let (accepted, refused) = if first.status().is_success() {
        (first, second)
    } else {
        (second, first)
    };
    assert_tokens(accepted).await;
    assert_oauth_error(refused, "invalid_grant").await;
    assert_oauth_error(server.exchange_code(&client, &code).await, "invalid_grant").await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_fails_when_code_is_expired() {
    let server = Server::start().await;
    let client = server.register_client("Example client").await;
    let code = server.approve_code(&client, None).await;
    server
        .expire_oauth_credential("oauth_code", "expires_at")
        .await;
    assert_oauth_error(server.exchange_code(&client, &code).await, "invalid_grant").await;
    let fresh_code = server.approve_code(&client, None).await;
    assert_tokens(server.exchange_code(&client, &fresh_code).await).await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_fails_when_redirect_uri_differs() {
    let server = Server::start().await;
    let client = server.register_client("Example client").await;
    let code = server.approve_code(&client, None).await;
    let mut form = exchange_form(&client, &code);
    form[3].1 = "http://127.0.0.1:46138/callback";
    assert_oauth_error(server.token_request(&form).await, "invalid_grant").await;
    assert_tokens(server.exchange_code(&client, &code).await).await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_fails_when_agent_name_was_taken_after_consent() {
    let server = Server::start().await;
    let first = server.register_client("First client").await;
    let second = server.register_client("Second client").await;
    let first_code = server.approve_code(&first, Some("Build agent")).await;
    let second_code = server.approve_code(&second, Some("Build agent")).await;
    let tokens = assert_tokens(server.exchange_code(&first, &first_code).await).await;
    assert_oauth_error(
        server.exchange_code(&second, &second_code).await,
        "invalid_grant",
    )
    .await;
    let refreshed = assert_tokens(
        server
            .refresh_oauth(&first, tokens["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
    assert_eq!(claims(&tokens)["sid"], claims(&refreshed)["sid"]);
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_fails_when_client_differs() {
    let server = Server::start().await;
    let client = server.register_client("Example client").await;
    let other = server.register_client("Other client").await;
    let code = server.approve_code(&client, None).await;
    for wrong_client in [other.as_str(), CLI_CLIENT, "unknown-client"] {
        assert_oauth_error(
            server.exchange_code(wrong_client, &code).await,
            "invalid_grant",
        )
        .await;
    }
    assert_tokens(server.exchange_code(&client, &code).await).await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_replaces_old_authorization_when_person_chose_to_replace() {
    let server = Server::start().await;
    let old = server.authorize_agent(AGENT_NAME).await;
    let client = server.register_client(AGENT_NAME).await;
    let person = server.tokens().await;
    let code = replacement_code(&server, &client, &person).await;
    let old_tokens = assert_tokens(
        server
            .refresh_oauth(
                &old.client_id,
                old.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
    )
    .await;
    let new = assert_tokens(server.exchange_code(&client, &code).await).await;
    assert_ne!(new["access_token"], old_tokens["access_token"]);
    assert_oauth_error(
        server
            .refresh_oauth(
                &old.client_id,
                old_tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
        "invalid_grant",
    )
    .await;
    assert_tokens(
        server
            .refresh_oauth(&client, new["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
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
    assert_oauth_error(
        server
            .token_request(&[
                ("grant_type", "authorization_code"),
                ("client_id", &client),
                ("code", &code),
                ("redirect_uri", REDIRECT_URI),
                ("code_verifier", &PKCE_VERIFIER.repeat(2)),
            ])
            .await,
        "invalid_grant",
    )
    .await;
    assert_tokens(
        server
            .refresh_oauth(
                &old.client_id,
                old.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
    )
    .await;
    assert_tokens(server.exchange_code(&client, &code).await).await;
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
    let tokens = assert_tokens(server.exchange_code(&client, &first).await).await;
    assert_oauth_error(
        server.exchange_code(&client, &second).await,
        "invalid_grant",
    )
    .await;
    assert_tokens(
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
    let new = assert_tokens(server.exchange_code(&client, &code).await).await;
    assert_oauth_error(
        server
            .refresh_oauth(
                &old.client_id,
                old.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
        "invalid_grant",
    )
    .await;
    assert_tokens(
        server
            .refresh_oauth(&client, new["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
    assert_taken(submit(&server, decision(&client), &person).await).await;
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_returns_new_tokens() {
    let server =
        Server::start_with_settings(Settings::builder().refresh_seconds(3_u64).build()).await;
    let agent = server.authorize_agent("Build agent").await;
    let oldest_refresh = agent.tokens["refresh_token"].as_str().unwrap().to_owned();
    let mut tokens = agent.tokens;
    for _ in 0..2 {
        sleep(RENEWAL_WAIT).await;
        let refreshed = assert_tokens(
            server
                .refresh_oauth(&agent.client_id, tokens["refresh_token"].as_str().unwrap())
                .await,
        )
        .await;
        assert_ne!(refreshed["refresh_token"], tokens["refresh_token"]);
        assert_ne!(refreshed["access_token"], tokens["access_token"]);
        assert_eq!(claims(&refreshed)["sid"], claims(&tokens)["sid"]);
        assert_eq!(claims(&refreshed)["kind"], "agent_authorization");
        tokens = refreshed;
    }
    assert_oauth_error(
        server
            .refresh_oauth(&agent.client_id, &oldest_refresh)
            .await,
        "invalid_grant",
    )
    .await;
    assert_tokens(
        server
            .refresh_oauth(&agent.client_id, tokens["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_ends_agent_authorization_when_refresh_token_was_already_exchanged() {
    let server = Server::start().await;
    let agent = server.authorize_agent("Build agent").await;
    let other = server.register_client("Other client").await;
    let mut tokens = agent.tokens;
    for client in [
        agent.client_id.as_str(),
        other.as_str(),
        CLI_CLIENT,
        "unknown-client",
    ] {
        let refreshed = assert_tokens(
            server
                .refresh_oauth(&agent.client_id, tokens["refresh_token"].as_str().unwrap())
                .await,
        )
        .await;
        assert_oauth_error(
            server
                .refresh_oauth(client, tokens["refresh_token"].as_str().unwrap())
                .await,
            "invalid_grant",
        )
        .await;
        assert_oauth_error(
            server
                .refresh_oauth(
                    &agent.client_id,
                    refreshed["refresh_token"].as_str().unwrap(),
                )
                .await,
            "invalid_grant",
        )
        .await;
        let code = server
            .approve_code(&agent.client_id, Some("Build agent"))
            .await;
        tokens = assert_tokens(server.exchange_code(&agent.client_id, &code).await).await;
        assert_ne!(claims(&tokens)["sid"], claims(&refreshed)["sid"]);
    }
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_renews_login_session_when_client_is_cli() {
    let server =
        Server::start_with_settings(Settings::builder().refresh_seconds(3_u64).build()).await;
    let code = server.approve_code(CLI_CLIENT, None).await;
    let mut tokens = assert_tokens(server.exchange_code(CLI_CLIENT, &code).await).await;
    let oldest_refresh = tokens["refresh_token"].as_str().unwrap().to_owned();
    for _ in 0..2 {
        sleep(RENEWAL_WAIT).await;
        let refreshed = assert_tokens(
            server
                .refresh_oauth(CLI_CLIENT, tokens["refresh_token"].as_str().unwrap())
                .await,
        )
        .await;
        assert_ne!(refreshed["refresh_token"], tokens["refresh_token"]);
        assert_ne!(refreshed["access_token"], tokens["access_token"]);
        assert_eq!(claims(&refreshed)["sid"], claims(&tokens)["sid"]);
        assert_person(&server, &refreshed).await;
        tokens = refreshed;
    }
    assert_oauth_error(
        server.refresh_oauth(CLI_CLIENT, &oldest_refresh).await,
        "invalid_grant",
    )
    .await;
    assert_person(&server, &tokens).await;
    assert_tokens(
        server
            .refresh_oauth(CLI_CLIENT, tokens["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_ends_login_session_when_cli_refresh_token_was_already_exchanged() {
    let server = Server::start().await;
    let other = server.register_client("Other client").await;
    let independent = server.tokens().await;
    for client in [CLI_CLIENT, other.as_str(), "unknown-client"] {
        let code = server.approve_code(CLI_CLIENT, None).await;
        let tokens = assert_tokens(server.exchange_code(CLI_CLIENT, &code).await).await;
        let refreshed = assert_tokens(
            server
                .refresh_oauth(CLI_CLIENT, tokens["refresh_token"].as_str().unwrap())
                .await,
        )
        .await;
        assert_person(&server, &refreshed).await;
        assert_oauth_error(
            server
                .refresh_oauth(client, tokens["refresh_token"].as_str().unwrap())
                .await,
            "invalid_grant",
        )
        .await;
        assert_eq!(
            server
                .log_out(refreshed["access_token"].as_str())
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_oauth_error(
            server
                .refresh_oauth(CLI_CLIENT, refreshed["refresh_token"].as_str().unwrap())
                .await,
            "invalid_grant",
        )
        .await;
        assert_person(&server, &independent).await;
    }
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_ends_login_session_when_refreshes_race_across_endpoints() {
    let server = Server::start().await;
    let code = server.approve_code(CLI_CLIENT, None).await;
    let tokens = assert_tokens(server.exchange_code(CLI_CLIENT, &code).await).await;
    let refresh = tokens["refresh_token"].as_str().unwrap();
    let (web, oauth) = tokio::join!(
        server.refresh(refresh),
        server.refresh_oauth(CLI_CLIENT, refresh),
    );
    let renewed = if web.status() == StatusCode::OK {
        assert_oauth_error(oauth, "invalid_grant").await;
        web.json::<Value>().await.unwrap()["data"].clone()
    } else {
        assert_eq!(web.status(), StatusCode::UNAUTHORIZED);
        assert_tokens(oauth).await
    };
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    for credentials in [&tokens, &renewed] {
        assert_eq!(
            server
                .log_out(credentials["access_token"].as_str())
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        server
            .refresh(renewed["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_oauth_error(
        server
            .refresh_oauth(CLI_CLIENT, renewed["refresh_token"].as_str().unwrap())
            .await,
        "invalid_grant",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_fails_when_agent_authorization_is_expired() {
    let server = Server::start().await;
    let agent = server.authorize_agent("Build agent").await;
    server
        .expire_oauth_credential("agent_authorization", "refresh_expires_at")
        .await;
    assert_oauth_error(
        server
            .refresh_oauth(
                &agent.client_id,
                agent.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
        "invalid_grant",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_fails_when_client_differs() {
    let server = Server::start().await;
    let agent = server.authorize_agent("Build agent").await;
    let other = server.register_client("Other client").await;
    let refresh = agent.tokens["refresh_token"].as_str().unwrap();
    for client in [other.as_str(), CLI_CLIENT, "unknown-client"] {
        assert_oauth_error(server.refresh_oauth(client, refresh).await, "invalid_grant").await;
    }
    assert_tokens(server.refresh_oauth(&agent.client_id, refresh).await).await;
    let code = server.approve_code(CLI_CLIENT, None).await;
    let tokens = assert_tokens(server.exchange_code(CLI_CLIENT, &code).await).await;
    let refresh = tokens["refresh_token"].as_str().unwrap();
    assert_oauth_error(
        server.refresh_oauth(&agent.client_id, refresh).await,
        "invalid_grant",
    )
    .await;
    let renewed = assert_tokens(server.refresh_oauth(CLI_CLIENT, refresh).await).await;
    assert_person(&server, &renewed).await;
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_fails_when_login_session_is_expired() {
    let server = Server::start().await;
    let code = server.approve_code(CLI_CLIENT, None).await;
    let tokens = assert_tokens(server.exchange_code(CLI_CLIENT, &code).await).await;
    server.expire_login_sessions().await;
    assert_oauth_error(
        server
            .refresh_oauth(CLI_CLIENT, tokens["refresh_token"].as_str().unwrap())
            .await,
        "invalid_grant",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn token_grants_fail_when_resource_is_not_lorebay() {
    let server = Server::start().await;
    let client = server.register_client("Example client").await;
    let code = server.approve_code(&client, None).await;
    let mut form = exchange_form(&client, &code);
    form.push(("resource", "https://untrusted.invalid/mcp"));
    assert_oauth_error(server.token_request(&form).await, "invalid_target").await;
    form.pop();
    form.push(("resource", ""));
    assert_oauth_error(server.token_request(&form).await, "invalid_target").await;
    let public_url = format!("http://{}", server.address);
    form.pop();
    form.push(("resource", &public_url));
    let tokens = assert_tokens(server.token_request(&form).await).await;
    let mut refresh = vec![
        ("grant_type", "refresh_token"),
        ("client_id", client.as_str()),
        ("refresh_token", tokens["refresh_token"].as_str().unwrap()),
        ("resource", "https://untrusted.invalid/mcp"),
    ];
    assert_oauth_error(server.token_request(&refresh).await, "invalid_target").await;
    refresh.pop();
    refresh.push(("resource", &public_url));
    let renewed = assert_tokens(server.token_request(&refresh).await).await;
    assert_tokens(
        server
            .token_request(&[
                ("grant_type", "refresh_token"),
                ("client_id", &client),
                ("refresh_token", renewed["refresh_token"].as_str().unwrap()),
            ])
            .await,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_fails_when_required_parameters_are_invalid() {
    let server = Server::start().await;
    let client = server.register_client("Example client").await;
    let code = server.approve_code(&client, None).await;
    for field in 0..5 {
        let mut form = exchange_form(&client, &code);
        form.remove(field);
        assert_oauth_error(server.token_request(&form).await, "invalid_request").await;
    }
    for verifier in [
        "too-short",
        "invalid-character-verifier-that-is-long-enough!",
    ] {
        let mut form = exchange_form(&client, &code);
        form[4].1 = verifier;
        assert_oauth_error(server.token_request(&form).await, "invalid_request").await;
    }
    let mut duplicate = exchange_form(&client, &code);
    duplicate.push(("client_id", &client));
    assert_oauth_error(server.token_request(&duplicate).await, "invalid_request").await;
    assert_tokens(server.token_request(&exchange_form(&client, &code)).await).await;
    server.stop().await;
}

#[tokio::test]
async fn token_grant_fails_when_grant_type_is_unsupported() {
    let server = Server::start().await;
    let client = server.register_client("Example client").await;
    assert_oauth_error(
        server
            .token_request(&[("grant_type", "client_credentials"), ("client_id", &client)])
            .await,
        "unsupported_grant_type",
    )
    .await;
    server.stop().await;
}

async fn assert_tokens(response: Response) -> Value {
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["pragma"], "no-cache");
    assert_eq!(response.headers()["content-type"], "application/json");
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(body.as_object().unwrap().len(), 4);
    assert_eq!(body["token_type"], "Bearer");
    assert!(body["expires_in"].as_u64().unwrap() > 0);
    assert_eq!(body["refresh_token"].as_str().unwrap().len(), 44);
    assert_eq!(body["access_token"].as_str().unwrap().split('.').count(), 3);
    body
}

async fn assert_oauth_error(response: Response, expected: &str) {
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["pragma"], "no-cache");
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(body["error"], expected);
    assert!(
        body["error_description"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(body.as_object().unwrap().len(), 2);
}

fn claims(tokens: &Value) -> Value {
    let payload = tokens["access_token"]
        .as_str()
        .unwrap()
        .split('.')
        .nth(1)
        .unwrap();
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
}

fn exchange_form<'a>(client: &'a str, code: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("grant_type", "authorization_code"),
        ("client_id", client),
        ("code", code),
        ("redirect_uri", REDIRECT_URI),
        ("code_verifier", PKCE_VERIFIER),
    ]
}

async fn assert_person(server: &Server, tokens: &Value) {
    let response = server.json(
        Method::POST,
        "/api/v1/authorization-request",
        json!({"client_id": CLI_CLIENT, "redirect_uri": REDIRECT_URI,
            "response_type": "code", "code_challenge": "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
            "code_challenge_method": "S256"}),
        tokens["access_token"].as_str(),
    ).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap()["data"]["default_agent_name"],
        Value::Null
    );
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

#[tokio::test]
async fn refresh_token_ends_agent_authorization_when_older_replaced_refresh_token_is_presented() {
    let server = Server::start().await;
    let agent = server.authorize_agent(AGENT_NAME).await;
    let independent = server.authorize_agent("Independent agent").await;
    let newest = refresh_twice(&server, &agent.client_id, &agent.tokens).await;
    assert_oauth_error(
        server
            .refresh_oauth(
                "unknown-client",
                agent.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
        "invalid_grant",
    )
    .await;
    assert_ended(&server, &agent.client_id, &newest).await;
    assert_live(&server, &independent.client_id, &independent.tokens).await;
    let code = server
        .approve_code(&agent.client_id, Some(AGENT_NAME))
        .await;
    assert_tokens(server.exchange_code(&agent.client_id, &code).await).await;
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_ends_login_session_when_older_replaced_cli_refresh_token_is_presented() {
    let server = Server::start().await;
    let code = server.approve_code(CLI_CLIENT, None).await;
    let original = assert_tokens(server.exchange_code(CLI_CLIENT, &code).await).await;
    let response = server
        .refresh(original["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let refreshed = response.json::<Value>().await.unwrap()["data"].clone();
    let newest = assert_tokens(
        server
            .refresh_oauth(CLI_CLIENT, refreshed["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
    assert_oauth_error(
        server
            .refresh_oauth(
                "unknown-client",
                original["refresh_token"].as_str().unwrap(),
            )
            .await,
        "invalid_grant",
    )
    .await;
    assert_ended(&server, CLI_CLIENT, &newest).await;
    let code = server.approve_code(CLI_CLIENT, None).await;
    let fresh = assert_tokens(server.exchange_code(CLI_CLIENT, &code).await).await;
    assert_person(&server, &fresh).await;
    server.stop().await;
}

#[tokio::test]
async fn refresh_login_session_ends_agent_authorization_when_replaced_refresh_token_is_presented() {
    let server = Server::start().await;
    let agent = server.authorize_agent(AGENT_NAME).await;
    let independent = server.authorize_agent("Independent agent").await;
    assert_eq!(
        server
            .refresh(agent.tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let newest = refresh_twice(&server, &agent.client_id, &agent.tokens).await;
    let unknown = server.refresh("unknown-refresh-token").await;
    let replay = server
        .refresh(agent.tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        replay.headers()["content-type"],
        unknown.headers()["content-type"]
    );
    assert_eq!(
        replay.json::<Value>().await.unwrap(),
        unknown.json::<Value>().await.unwrap()
    );
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    assert_ended(&server, &agent.client_id, &newest).await;
    assert_live(&server, &independent.client_id, &independent.tokens).await;
    let code = server
        .approve_code(&agent.client_id, Some(AGENT_NAME))
        .await;
    assert_tokens(server.exchange_code(&agent.client_id, &code).await).await;
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_fails_without_ending_agent_authorization_when_replaced_refresh_token_has_expired()
 {
    let server = Server::start().await;
    let agent = server.authorize_agent(AGENT_NAME).await;
    let newest = refresh_twice(&server, &agent.client_id, &agent.tokens).await;
    server
        .expire_oauth_credential("refresh_token_history", "expires_at")
        .await;
    assert_oauth_error(
        server
            .refresh_oauth(
                &agent.client_id,
                agent.tokens["refresh_token"].as_str().unwrap(),
            )
            .await,
        "invalid_grant",
    )
    .await;
    assert_eq!(
        server
            .refresh(agent.tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_live(&server, &agent.client_id, &newest).await;
    server.stop().await;
}

#[tokio::test]
async fn refresh_token_fails_without_ending_login_session_when_replaced_refresh_token_has_expired()
{
    let server = Server::start().await;
    let original = server.tokens().await;
    let newest = refresh_twice(&server, CLI_CLIENT, &original).await;
    server
        .expire_oauth_credential("refresh_token_history", "expires_at")
        .await;
    assert_eq!(
        server
            .refresh(original["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_oauth_error(
        server
            .refresh_oauth(CLI_CLIENT, original["refresh_token"].as_str().unwrap())
            .await,
        "invalid_grant",
    )
    .await;
    assert_live(&server, CLI_CLIENT, &newest).await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_ends_agent_authorization_when_code_was_already_exchanged() {
    let server = Server::start().await;
    let client = server.register_client(AGENT_NAME).await;
    let code = server.approve_code(&client, Some(AGENT_NAME)).await;
    let original = assert_tokens(server.exchange_code(&client, &code).await).await;
    let newest = refresh_twice(&server, &client, &original).await;
    assert_oauth_error(server.exchange_code(&client, &code).await, "invalid_grant").await;
    assert_ended(&server, &client, &original).await;
    assert_ended(&server, &client, &newest).await;
    let fresh_code = server.approve_code(&client, Some(AGENT_NAME)).await;
    let fresh = assert_tokens(server.exchange_code(&client, &fresh_code).await).await;
    assert_oauth_error(server.exchange_code(&client, &code).await, "invalid_grant").await;
    assert_live(&server, &client, &fresh).await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_ends_login_session_when_cli_code_was_already_exchanged() {
    let server = Server::start().await;
    let code = server.approve_code(CLI_CLIENT, None).await;
    let original = assert_tokens(server.exchange_code(CLI_CLIENT, &code).await).await;
    let newest = refresh_twice(&server, CLI_CLIENT, &original).await;
    let independent = server.tokens().await;
    assert_oauth_error(
        server.exchange_code(CLI_CLIENT, &code).await,
        "invalid_grant",
    )
    .await;
    assert_ended(&server, CLI_CLIENT, &original).await;
    assert_ended(&server, CLI_CLIENT, &newest).await;
    assert_oauth_error(
        server.exchange_code(CLI_CLIENT, &code).await,
        "invalid_grant",
    )
    .await;
    assert_live(&server, CLI_CLIENT, &independent).await;
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_keeps_agent_authorization_when_replay_verifier_is_wrong() {
    let server = Server::start().await;
    let client = server.register_client(AGENT_NAME).await;
    for client in [client.as_str(), CLI_CLIENT] {
        let code = server.approve_code(client, None).await;
        let tokens = assert_tokens(server.exchange_code(client, &code).await).await;
        let mut form = exchange_form(client, &code);
        form[4].1 = "wrong-verifier-that-is-still-valid-and-long-enough";
        assert_oauth_error(server.token_request(&form).await, "invalid_grant").await;
        assert_live(&server, client, &tokens).await;
    }
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_keeps_authorization_when_replay_binding_differs() {
    let server = Server::start().await;
    let client = server.register_client(AGENT_NAME).await;
    let other = server.register_client("Other client").await;
    for client in [client.as_str(), CLI_CLIENT] {
        let code = server.approve_code(client, None).await;
        let tokens = assert_tokens(server.exchange_code(client, &code).await).await;
        for (field, value) in [
            (1, other.as_str()),
            (1, "unknown-client"),
            (3, "http://127.0.0.1:46138/callback"),
        ] {
            let mut form = exchange_form(client, &code);
            form[field].1 = value;
            assert_oauth_error(server.token_request(&form).await, "invalid_grant").await;
        }
        assert_live(&server, client, &tokens).await;
    }
    server.stop().await;
}

#[tokio::test]
async fn exchange_code_keeps_authorization_when_exchanged_code_has_expired() {
    let server = Server::start().await;
    let client = server.register_client(AGENT_NAME).await;
    for client in [client.as_str(), CLI_CLIENT] {
        let code = server.approve_code(client, None).await;
        let tokens = assert_tokens(server.exchange_code(client, &code).await).await;
        server
            .expire_oauth_credential("oauth_code", "expires_at")
            .await;
        assert_oauth_error(server.exchange_code(client, &code).await, "invalid_grant").await;
        assert_live(&server, client, &tokens).await;
    }
    server.stop().await;
}

async fn refresh_twice(server: &Server, client: &str, original: &Value) -> Value {
    let mut tokens = original.clone();
    for _ in 0..2 {
        tokens = assert_tokens(
            server
                .refresh_oauth(client, tokens["refresh_token"].as_str().unwrap())
                .await,
        )
        .await;
    }
    tokens
}

async fn assert_ended(server: &Server, client: &str, tokens: &Value) {
    assert_eq!(
        server
            .json(
                Method::GET,
                "/api/v1/agents",
                Value::Null,
                tokens["access_token"].as_str()
            )
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_oauth_error(
        server
            .refresh_oauth(client, tokens["refresh_token"].as_str().unwrap())
            .await,
        "invalid_grant",
    )
    .await;
    if client == CLI_CLIENT {
        assert_eq!(
            server
                .refresh(tokens["refresh_token"].as_str().unwrap())
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
}

async fn assert_live(server: &Server, client: &str, tokens: &Value) {
    let expected = if client == CLI_CLIENT {
        StatusCode::OK
    } else {
        StatusCode::FORBIDDEN
    };
    assert_eq!(
        server
            .json(
                Method::GET,
                "/api/v1/agents",
                Value::Null,
                tokens["access_token"].as_str()
            )
            .await
            .status(),
        expected
    );
    assert_tokens(
        server
            .refresh_oauth(client, tokens["refresh_token"].as_str().unwrap())
            .await,
    )
    .await;
}
