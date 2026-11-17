use std::time::Duration;

use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::helpers::{Server, Settings};

mod tools;

#[tokio::test]
async fn call_mcp_fails_with_authorization_challenge_when_token_is_missing() {
    let server = Server::start().await;
    for method in [Method::POST, Method::GET] {
        let response = server
            .client
            .request(method, format!("http://{}/mcp", server.address))
            .header(VERSION_HEADER, "2024-11-05")
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
            .send()
            .await
            .unwrap();
        assert_challenge(response, &server).await;
    }
    server.stop().await;
}

#[tokio::test]
async fn list_tools_returns_six_tools_in_fixed_order() {
    let server = Server::start().await;
    let agent = server.authorize_agent("MCP agent").await;
    let token = agent.tokens["access_token"].as_str().unwrap();
    let discovery = mcp_call(&server, "server/discover", NEWEST_VERSION, token).await;
    assert_eq!(discovery.status(), StatusCode::OK);
    assert_eq!(discovery.headers()["content-type"], "application/json");
    assert!(!discovery.headers().contains_key(SESSION_HEADER));
    let body = discovery.json::<Value>().await.unwrap();
    assert_eq!(
        body["result"]["supportedVersions"],
        json!(["2025-03-26", "2025-06-18", "2025-11-25", NEWEST_VERSION])
    );
    assert_eq!(body["result"]["capabilities"]["tools"], json!({}));
    assert_eq!(
        body["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "lorebayd"
    );
    assert_tools(
        NEWEST_VERSION,
        mcp_call(&server, "tools/list", NEWEST_VERSION, token).await,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn initialize_succeeds_without_session_when_protocol_version_is_older() {
    let server = Server::start().await;
    let agent = server.authorize_agent("MCP agent").await;
    let token = agent.tokens["access_token"].as_str().unwrap();
    for version in ["2025-03-26", "2025-06-18", "2025-11-25"] {
        let response = server
            .client
            .post(format!("http://{}/mcp", server.address))
            .bearer_auth(token)
            .header("accept", ACCEPT_TYPES)
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion": version, "capabilities": {},
                    "clientInfo": {"name": "integration", "version": "1"}}}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(!response.headers().contains_key(SESSION_HEADER));
        let body = response.json::<Value>().await.unwrap();
        assert_eq!(body["result"]["protocolVersion"], version);
        assert_eq!(body["result"]["capabilities"]["tools"], json!({}));
        notify_initialized(&server, version, token).await;
        assert_tools(
            version,
            mcp_call(&server, "tools/list", version, token).await,
        )
        .await;
    }
    for method in [Method::GET, Method::DELETE] {
        let response = server
            .client
            .request(method, format!("http://{}/mcp", server.address))
            .bearer_auth(token)
            .header(VERSION_HEADER, "2025-06-18")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(response.headers()["allow"], "POST");
        assert!(!response.headers().contains_key(SESSION_HEADER));
    }
    server.stop().await;
}

#[tokio::test]
async fn call_mcp_fails_when_protocol_version_is_not_supported() {
    let server = Server::start().await;
    let agent = server.authorize_agent("MCP agent").await;
    for version in ["2024-11-05", "2099-01-01"] {
        let response = mcp_call(
            &server,
            "tools/list",
            version,
            agent.tokens["access_token"].as_str().unwrap(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(!response.headers().contains_key("www-authenticate"));
    }
    server.stop().await;
}

#[tokio::test]
async fn call_mcp_fails_when_access_token_is_expired() {
    let server =
        Server::start_with_settings(Settings::builder().access_seconds(2_u64).build()).await;
    let agent = server.authorize_agent("MCP agent").await;
    let token = agent.tokens["access_token"].as_str().unwrap();
    assert_tools(
        NEWEST_VERSION,
        mcp_call(&server, "tools/list", NEWEST_VERSION, token).await,
    )
    .await;
    // Refreshed tokens need the default lifetime to stay valid for the final call on slow runners.
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    sleep(Duration::from_secs(3)).await;
    assert_challenge(
        mcp_call(&server, "tools/list", NEWEST_VERSION, token).await,
        &server,
    )
    .await;
    let refreshed = server
        .refresh_oauth(
            &agent.client_id,
            agent.tokens["refresh_token"].as_str().unwrap(),
        )
        .await;
    assert_eq!(refreshed.status(), StatusCode::OK);
    let tokens = refreshed.json::<Value>().await.unwrap();
    assert_tools(
        NEWEST_VERSION,
        mcp_call(
            &server,
            "tools/list",
            NEWEST_VERSION,
            tokens["access_token"].as_str().unwrap(),
        )
        .await,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn call_mcp_fails_when_token_belongs_to_login_session() {
    let server = Server::start().await;
    let tokens = server.tokens().await;
    assert_challenge(
        mcp_call(
            &server,
            "tools/list",
            NEWEST_VERSION,
            tokens["access_token"].as_str().unwrap(),
        )
        .await,
        &server,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn call_mcp_fails_when_account_was_removed_from_users_file() {
    let server = Server::start().await;
    let agent = server.authorize_agent("MCP agent").await;
    let token = agent.tokens["access_token"].as_str().unwrap();
    assert_tools(
        NEWEST_VERSION,
        mcp_call(&server, "tools/list", NEWEST_VERSION, token).await,
    )
    .await;
    let directory = server.stop().await;
    let server = Server::restart_with_users(directory, &[]).await;
    assert_challenge(
        mcp_call(&server, "tools/list", NEWEST_VERSION, token).await,
        &server,
    )
    .await;
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    assert_challenge(
        mcp_call(&server, "tools/list", NEWEST_VERSION, token).await,
        &server,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn call_mcp_fails_when_authorization_is_expired() {
    let server = Server::start().await;
    let agent = server.authorize_agent("MCP agent").await;
    let token = agent.tokens["access_token"].as_str().unwrap();
    assert_tools(
        NEWEST_VERSION,
        mcp_call(&server, "tools/list", NEWEST_VERSION, token).await,
    )
    .await;
    server
        .expire_oauth_credential("agent_authorization", "refresh_expires_at")
        .await;
    assert_challenge(
        mcp_call(&server, "tools/list", NEWEST_VERSION, token).await,
        &server,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn call_mcp_fails_when_authorization_was_removed() {
    let server = Server::start().await;
    let agent = server.authorize_agent("MCP agent").await;
    let token = agent.tokens["access_token"].as_str().unwrap();
    assert_tools(
        NEWEST_VERSION,
        mcp_call(&server, "tools/list", NEWEST_VERSION, token).await,
    )
    .await;
    server.remove_agent_authorizations().await;
    assert_challenge(
        mcp_call(&server, "tools/list", NEWEST_VERSION, token).await,
        &server,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn call_mcp_fails_when_token_signature_is_invalid() {
    let server = Server::start().await;
    let agent = server.authorize_agent("MCP agent").await;
    let token = agent.tokens["access_token"].as_str().unwrap();
    let (payload, signature) = token.rsplit_once('.').unwrap();
    let replacement = if signature.starts_with('A') { "B" } else { "A" };
    let forged = format!("{payload}.{replacement}{}", &signature[1..]);
    assert_challenge(
        mcp_call(&server, "tools/list", NEWEST_VERSION, &forged).await,
        &server,
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn call_mcp_succeeds_when_proxy_rewrites_host() {
    let server = Server::start().await;
    let agent = server.authorize_agent("MCP agent").await;
    let response = server
        .client
        .post(format!("http://{}/mcp", server.address))
        .bearer_auth(agent.tokens["access_token"].as_str().unwrap())
        .header("host", "lorebay.test:8080")
        .header("origin", format!("http://{}", server.address))
        .header("accept", ACCEPT_TYPES)
        .header(VERSION_HEADER, NEWEST_VERSION)
        .header("mcp-method", "tools/list")
        .json(&request_body("tools/list", NEWEST_VERSION))
        .send()
        .await
        .unwrap();
    assert_tools(NEWEST_VERSION, response).await;
    server.stop().await;
}

#[tokio::test]
async fn call_mcp_fails_when_origin_differs_from_public_url() {
    let server = Server::start().await;
    let agent = server.authorize_agent("MCP agent").await;
    let token = agent.tokens["access_token"].as_str().unwrap();
    let origin = format!("http://{}", server.address);
    for (origin, expected) in [
        (origin.as_str(), StatusCode::OK),
        ("https://untrusted.invalid", StatusCode::FORBIDDEN),
        ("null", StatusCode::FORBIDDEN),
        ("http://127.0.0.1:1", StatusCode::FORBIDDEN),
        ("https://127.0.0.1", StatusCode::FORBIDDEN),
    ] {
        let response = server
            .client
            .post(format!("http://{}/mcp", server.address))
            .bearer_auth(token)
            .header("origin", origin)
            .header("accept", ACCEPT_TYPES)
            .header(VERSION_HEADER, NEWEST_VERSION)
            .header("mcp-method", "tools/list")
            .json(&request_body("tools/list", NEWEST_VERSION))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        if expected == StatusCode::OK {
            assert_tools(NEWEST_VERSION, response).await;
        }
    }
    server.stop().await;
}

const NEWEST_VERSION: &str = "2026-07-28";
const VERSION_HEADER: &str = "mcp-protocol-version";
const SESSION_HEADER: &str = "mcp-session-id";
const ACCEPT_TYPES: &str = "application/json, text/event-stream";

fn request_body(method: &str, version: &str) -> Value {
    let params = if version == NEWEST_VERSION {
        json!({"_meta": {
            "io.modelcontextprotocol/protocolVersion": version,
            "io.modelcontextprotocol/clientInfo": {"name": "integration", "version": "1"},
            "io.modelcontextprotocol/clientCapabilities": {}
        }})
    } else {
        json!({})
    };
    json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
}

async fn mcp_call(server: &Server, method: &str, version: &str, token: &str) -> Response {
    server
        .client
        .post(format!("http://{}/mcp", server.address))
        .bearer_auth(token)
        .header("accept", ACCEPT_TYPES)
        .header(VERSION_HEADER, version)
        .header("mcp-method", method)
        .json(&request_body(method, version))
        .send()
        .await
        .unwrap()
}

async fn notify_initialized(server: &Server, version: &str, token: &str) {
    let response = server
        .client
        .post(format!("http://{}/mcp", server.address))
        .bearer_auth(token)
        .header("accept", ACCEPT_TYPES)
        .header(VERSION_HEADER, version)
        .json(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert!(!response.headers().contains_key(SESSION_HEADER));
}

async fn assert_challenge(response: Response, server: &Server) {
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers()["www-authenticate"],
        format!(
            "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\"",
            server.public_url.trim_end_matches('/')
        )
    );
    assert!(!response.headers().contains_key(SESSION_HEADER));
    let metadata = server
        .get("/.well-known/oauth-protected-resource/mcp")
        .await
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(
        metadata["resource"],
        format!("{}/mcp", server.public_url.trim_end_matches('/'))
    );
}

async fn assert_tools(version: &str, response: Response) {
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert!(!response.headers().contains_key(SESSION_HEADER));
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(body["jsonrpc"], "2.0");
    assert_eq!(body["id"], 1);
    assert!(body.get("error").is_none());
    let tools = body["result"]["tools"].as_array().unwrap();
    let expected = [
        ("list_spaces", true),
        ("create_task", false),
        ("update_task", false),
        ("list_tasks", true),
        ("get_task", true),
        ("add_note", false),
    ];
    assert_eq!(tools.len(), expected.len());
    for (tool, (name, read_only)) in tools.iter().zip(expected) {
        assert_eq!(tool["name"], name);
        assert!(!tool["description"].as_str().unwrap().is_empty());
        assert_eq!(tool["inputSchema"]["type"], "object");
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert_eq!(tool["annotations"]["readOnlyHint"], read_only);
    }
    if version == NEWEST_VERSION {
        assert_eq!(body["result"]["resultType"], "complete");
        assert_eq!(body["result"]["ttlMs"], 0);
        assert_eq!(body["result"]["cacheScope"], "private");
    } else {
        assert!(body["result"].get("resultType").is_none());
    }
}
