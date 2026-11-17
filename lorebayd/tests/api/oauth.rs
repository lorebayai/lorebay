use std::collections::HashSet;
use std::time::Duration;

use reqwest::{Response, StatusCode};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::helpers::Server;

#[tokio::test]
async fn get_authorization_server_metadata_returns_endpoints_under_public_url() {
    let server = Server::configured(|config| {
        config.public_url = "https://lorebay.test:8443/".to_owned();
    })
    .await;
    let response = server
        .client
        .get(format!(
            "http://{}/.well-known/oauth-authorization-server",
            server.address
        ))
        .header("host", "untrusted.invalid")
        .header("forwarded", "host=untrusted.invalid;proto=http")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({
            "issuer": "https://lorebay.test:8443",
            "authorization_endpoint": "https://lorebay.test:8443/oauth/authorize",
            "token_endpoint": "https://lorebay.test:8443/oauth/token",
            "registration_endpoint": "https://lorebay.test:8443/oauth/register",
            "device_authorization_endpoint": "https://lorebay.test:8443/oauth/device",
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code", "refresh_token", "urn:ietf:params:oauth:grant-type:device_code"],
            "code_challenge_methods_supported": ["S256"],
            "token_endpoint_auth_methods_supported": ["none"],
            "authorization_response_iss_parameter_supported": true
        })
    );
    server.stop().await;
}

#[tokio::test]
async fn get_protected_resource_metadata_names_authorization_server() {
    let server = Server::start().await;
    let response = server
        .get("/.well-known/oauth-protected-resource/mcp")
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({
            "resource": format!("http://{}/mcp", server.address),
            "authorization_servers": [format!("http://{}", server.address)]
        })
    );
    server.stop().await;
}

#[tokio::test]
async fn register_client_returns_client_id() {
    let server = Server::start().await;
    let mut identifiers = HashSet::new();
    for (name, redirect) in [
        ("Claude Code (lorebay)", "http://localhost:49644/callback"),
        ("Codex", "http://127.0.0.1:46137/callback"),
        ("Codex", "http://127.0.0.1:55433/callback/random/path"),
        ("IPv6 client", "http://[::1]:51434/any/path"),
        ("HTTPS client", "https://client.example/callback"),
    ] {
        let mut expected = registration(name, redirect);
        let response = register(&server, &expected).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(response.headers()["content-type"], "application/json");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["pragma"], "no-cache");
        let body = response.json::<Value>().await.unwrap();
        let client_id = body["client_id"].as_str().unwrap();
        assert_eq!(client_id.len(), 22);
        assert!(
            client_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        );
        assert!(identifiers.insert(client_id.to_owned()));
        expected["client_id"] = json!(client_id);
        assert_eq!(body, expected);
    }
    server.stop().await;
}

#[tokio::test]
async fn register_client_fails_when_redirect_uri_is_not_https_or_loopback() {
    let server = Server::start().await;
    for redirect in [
        "http://client.example/callback",
        "http://localhost.evil.invalid/callback",
        "http://192.0.2.1/callback",
        "ftp://localhost/callback",
        "not a URI",
        "https://client.example/callback#fragment",
        "https://user:password@client.example/callback",
    ] {
        let response = register(&server, &registration("Codex", redirect)).await;
        assert_error(
            response,
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "Redirect URIs must use HTTPS or HTTP on loopback, without credentials or fragments.",
        )
        .await;
    }
    server.stop().await;
}

#[tokio::test]
async fn register_client_fails_when_client_asks_for_a_secret() {
    let server = Server::start().await;
    for method in [
        "client_secret_basic",
        "client_secret_post",
        "private_key_jwt",
    ] {
        let mut body = registration("Codex", "http://127.0.0.1:46137/callback");
        body["token_endpoint_auth_method"] = json!(method);
        assert_error(
            register(&server, &body).await,
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            "Unsupported or invalid client metadata.",
        )
        .await;
    }
    server.stop().await;
}

#[tokio::test]
async fn register_client_fails_when_client_name_is_too_long() {
    let server = Server::start().await;
    for name in ["x".repeat(257), "\u{e9}".repeat(129)] {
        let body = registration(&name, "http://127.0.0.1:46137/callback");
        assert_error(
            register(&server, &body).await,
            StatusCode::BAD_REQUEST,
            "invalid_client_metadata",
            "Unsupported or invalid client metadata.",
        )
        .await;
    }
    let body = registration(&"\u{e9}".repeat(128), "http://127.0.0.1:46137/callback");
    assert_eq!(register(&server, &body).await.status(), StatusCode::CREATED);
    server.stop().await;
}

#[tokio::test]
async fn register_client_fails_when_registration_metadata_exceeds_limits() {
    let server = Server::start().await;
    let mut body = registration("Codex", "http://127.0.0.1:46137/callback");
    body["redirect_uris"] = json!(vec!["https://client.example/callback"; 5]);
    assert_error(
        register(&server, &body).await,
        StatusCode::BAD_REQUEST,
        "invalid_client_metadata",
        "Unsupported or invalid client metadata.",
    )
    .await;
    let prefix = "https://client.example/";
    body["redirect_uris"] = json!([format!("{prefix}{}", "x".repeat(2049 - prefix.len()))]);
    assert_error(
        register(&server, &body).await,
        StatusCode::BAD_REQUEST,
        "invalid_redirect_uri",
        "Redirect URIs must use HTTPS or HTTP on loopback, without credentials or fragments.",
    )
    .await;
    body = registration("Codex", "http://127.0.0.1:46137/callback");
    body["padding"] = json!("x".repeat(65_536));
    assert_error(
        register(&server, &body).await,
        StatusCode::PAYLOAD_TOO_LARGE,
        "invalid_client_metadata",
        "Expected a JSON client registration object.",
    )
    .await;
    body.as_object_mut().unwrap().remove("padding");
    body["client_name"] = json!("x".repeat(256));
    body["redirect_uris"] = json!(
        (0..4)
            .map(|_| format!("{prefix}{}", "x".repeat(2048 - prefix.len())))
            .collect::<Vec<_>>()
    );
    assert_eq!(register(&server, &body).await.status(), StatusCode::CREATED);
    server.stop().await;
}

#[tokio::test]
async fn register_client_fails_when_rate_limit_is_exceeded() {
    let server = Server::configured(|config| {
        config.registration_rate_limit = 1;
        config.registration_rate_window_seconds = 1;
    })
    .await;
    let invalid = registration("Codex", "http://client.example/callback");
    assert_eq!(
        register(&server, &invalid).await.status(),
        StatusCode::BAD_REQUEST
    );
    let body = registration("Codex", "http://127.0.0.1:46137/callback");
    let response = server
        .client
        .post(format!("http://{}/oauth/register", server.address))
        .header("x-forwarded-for", "192.0.2.1")
        .header("forwarded", "for=192.0.2.2")
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["retry-after"], "1");
    assert_error(
        response,
        StatusCode::TOO_MANY_REQUESTS,
        "slow_down",
        "Too many registration attempts; retry after the indicated wait.",
    )
    .await;
    sleep(Duration::from_secs(1)).await;
    assert_eq!(register(&server, &body).await.status(), StatusCode::CREATED);
    server.stop().await;
}

fn registration(name: &str, redirect: &str) -> Value {
    json!({
        "application_type": "native", "client_name": name,
        "grant_types": ["authorization_code", "refresh_token"],
        "redirect_uris": [redirect], "response_types": ["code"],
        "token_endpoint_auth_method": "none"
    })
}

async fn register(server: &Server, body: &Value) -> Response {
    server
        .client
        .post(format!("http://{}/oauth/register", server.address))
        .json(body)
        .send()
        .await
        .unwrap()
}

async fn assert_error(response: Response, status: StatusCode, error: &str, description: &str) {
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({
            "error": error, "error_description": description
        })
    );
}
