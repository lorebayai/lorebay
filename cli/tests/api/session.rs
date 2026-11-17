use std::fs::{self, File, OpenOptions};
use std::process::Output;
use std::time::Duration;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use tokio::time::{sleep, timeout};

use crate::helpers::{CLIENT_ID, Server, WAIT, assert_success, body};
use crate::helpers::{CREDENTIALS_FILE, LOGIN_SESSION_PATH};

#[tokio::test]
async fn agent_list_fails_with_login_hint_when_not_logged_in() {
    let server = Server::start().await;
    for mode in ["human", "json"] {
        assert_login_required(
            server.run(&["agent", "list", "--output", mode]).await,
            mode,
            "cli.login_required",
        );
    }
    server.login().await;
    let credentials = server.credentials();
    server
        .json(
            Method::DELETE,
            LOGIN_SESSION_PATH,
            Value::Null,
            credentials["access_token"].as_str(),
        )
        .await;
    assert_login_required(
        server.run(&["agent", "list", "--output", "json"]).await,
        "json",
        "invalid_grant",
    );
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
}

#[tokio::test]
async fn agent_list_refreshes_login_session_when_access_token_is_expired() {
    let mut server = Server::configured(json!({"access_token_lifetime_seconds": 2})).await;
    server.login().await;
    let before = server.credentials();
    server.set_access_lifetime(900).await;
    sleep(Duration::from_secs(3)).await;
    let output = server.run(&["agent", "list", "--output", "json"]).await;
    assert_success(&output);
    assert_eq!(body(&output), json!({"data": [], "meta": {}}));
    let after = server.credentials();
    assert!(before["refresh_token"] != after["refresh_token"]);
    assert!(before["access_token"] != after["access_token"]);
    assert_success(&server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn agent_list_refreshes_login_session_when_server_returns_unauthorized() {
    let mut server = Server::configured(json!({"access_token_lifetime_seconds": 2})).await;
    server.login().await;
    let mut saved = server.credentials();
    let original = saved["refresh_token"].clone();
    saved["expires_at"] = json!(u64::MAX);
    fs::write(server.home().join(CREDENTIALS_FILE), saved.to_string()).unwrap();
    server.set_access_lifetime(900).await;
    sleep(Duration::from_secs(3)).await;
    assert_success(&server.run(&["agent", "list"]).await);
    assert!(server.credentials()["refresh_token"] != original);
    assert_success(&server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn agent_list_keeps_login_session_when_two_commands_refresh_at_once() {
    let mut server = Server::configured(json!({"access_token_lifetime_seconds": 2})).await;
    server.login().await;
    server.set_access_lifetime(900).await;
    sleep(Duration::from_secs(3)).await;
    let first = server
        .cli(&["agent", "list", "--output", "json"])
        .spawn()
        .unwrap();
    let second = server
        .cli(&["agent", "list", "--output", "json"])
        .spawn()
        .unwrap();
    let (first, second) = timeout(WAIT, async {
        tokio::join!(first.wait_with_output(), second.wait_with_output())
    })
    .await
    .unwrap();
    assert_success(&first.unwrap());
    assert_success(&second.unwrap());
    assert_success(&server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn agent_list_preserves_credentials_when_server_cannot_be_reached() {
    let mut server = Server::start().await;
    server.login().await;
    let mut saved = server.credentials();
    saved["expires_at"] = json!(0);
    fs::write(server.home().join(CREDENTIALS_FILE), saved.to_string()).unwrap();
    let before = fs::read(server.home().join(CREDENTIALS_FILE)).unwrap();
    server.stop();
    let output = server.run(&["agent", "list", "--output", "json"]).await;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "cli.connection");
    assert!(fs::read(server.home().join(CREDENTIALS_FILE)).unwrap() == before);
}

#[tokio::test]
async fn agent_list_times_out_when_credentials_lock_is_held() {
    let server = Server::start().await;
    server.login().await;
    let mut saved = server.credentials();
    saved["expires_at"] = json!(0);
    fs::write(server.home().join(CREDENTIALS_FILE), saved.to_string()).unwrap();
    let before = fs::read(server.home().join(CREDENTIALS_FILE)).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(server.home().join("credentials.lock"))
        .unwrap();
    lock.lock().unwrap();
    let output = server.run(&["agent", "list", "--output", "json"]).await;
    assert_eq!(output.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "cli.timeout");
    assert!(fs::read(server.home().join(CREDENTIALS_FILE)).unwrap() == before);
    drop(lock);
    assert_success(&server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn logout_ends_login_session_and_removes_credentials() {
    let mut server = Server::start().await;
    server.login().await;
    let saved = server.credentials();
    let output = server.run(&["logout", "--output", "json"]).await;
    assert_success(&output);
    assert_eq!(body(&output), json!({"data": null, "meta": {}}));
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
    assert_eq!(
        server
            .client
            .get(format!("{}/api/v1/agents", server.url))
            .bearer_auth(saved["access_token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .client
            .post(format!("{}/oauth/token", server.url))
            .form(&[
                ("grant_type", "refresh_token"),
                ("client_id", CLIENT_ID),
                ("refresh_token", saved["refresh_token"].as_str().unwrap())
            ])
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_success(&server.run(&["logout"]).await);
    server.login().await;
    server.stop();
    let output = server.run(&["logout"]).await;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("session was not ended on the server")
    );
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
}

#[tokio::test]
async fn logout_succeeds_when_server_session_has_already_ended() {
    let server = Server::start().await;
    server.login().await;
    let saved = server.credentials();
    server
        .json(
            Method::DELETE,
            LOGIN_SESSION_PATH,
            Value::Null,
            saved["access_token"].as_str(),
        )
        .await;
    assert_success(&server.run(&["logout"]).await);
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
}

#[tokio::test]
async fn logout_reports_failure_when_credentials_cannot_be_removed() {
    let server = Server::start().await;
    server.login().await;
    let path = server.home().join(CREDENTIALS_FILE);
    let saved = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let output = server.run(&["logout", "--output", "json"]).await;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "cli.credentials");
    fs::remove_dir(path).unwrap();
    fs::write(server.home().join(CREDENTIALS_FILE), saved).unwrap();
    assert_success(&server.run(&["logout"]).await);
}

#[tokio::test]
async fn agent_list_reports_invalid_credentials_without_printing_secrets() {
    let server = Server::start().await;
    fs::create_dir_all(server.home()).unwrap();
    let path = server.home().join(CREDENTIALS_FILE);
    fs::write(path, "invalid credentials secret-value").unwrap();
    let output = server.run(&["agent", "list", "--output", "json"]).await;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("secret-value"));
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "cli.credentials");
    assert!(File::open(server.home().join(CREDENTIALS_FILE)).is_ok());
    let output = server.run(&["logout", "--output", "json"]).await;
    assert_eq!(output.status.code(), Some(1));
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
}

#[tokio::test]
async fn logout_refreshes_login_session_when_access_token_is_expired() {
    let mut server = Server::configured(json!({"access_token_lifetime_seconds": 2})).await;
    server.login().await;
    server.set_access_lifetime(900).await;
    sleep(Duration::from_secs(3)).await;
    assert_success(&server.run(&["logout", "--output", "json"]).await);
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
}

#[tokio::test]
async fn logout_preserves_server_error_when_caller_is_agent() {
    let server = Server::start().await;
    server.login().await;
    let original = server.credentials();
    let agent = server.agent("Caller agent").await;
    let mut saved = original.clone();
    saved["access_token"] = agent["access_token"].clone();
    fs::write(server.home().join(CREDENTIALS_FILE), saved.to_string()).unwrap();
    let output = server.run(&["logout", "--output", "json"]).await;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "auth.person_required");
    assert_eq!(error["title"], "A login session is required");
    assert_eq!(error["status"], 403);
    assert_eq!(error["errors"], json!([]));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("session was not ended on the server")
    );
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
    let response = server
        .client
        .get(format!("{}/api/v1/agents", server.url))
        .bearer_auth(original["access_token"].as_str().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn agent_list_keeps_login_session_when_two_commands_retry_unauthorized() {
    let mut server = Server::configured(json!({"access_token_lifetime_seconds": 2})).await;
    server.login().await;
    server.set_access_lifetime(900).await;
    let mut saved = server.credentials();
    saved["expires_at"] = json!(u64::MAX);
    fs::write(server.home().join(CREDENTIALS_FILE), saved.to_string()).unwrap();
    sleep(Duration::from_secs(3)).await;
    let (first, second) = tokio::join!(
        server.run(&["agent", "list"]),
        server.run(&["agent", "list"])
    );
    assert_success(&first);
    assert_success(&second);
    assert_success(&server.run(&["agent", "list"]).await);
}

fn assert_login_required(output: Output, mode: &str, expected_code: &str) {
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("lorebay login"));
    assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
    if expected_code == "invalid_grant" {
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(
                "The authorization grant is invalid, expired or belongs to another client."
            )
        );
    }
    if mode == "json" {
        assert!(
            serde_json::from_slice::<Value>(&output.stderr)
                .is_ok_and(|error| error["code"] == expected_code)
        );
    }
}
