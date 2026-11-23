use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Output;

use serde_json::{Value, json};

use crate::helpers::{
    CREDENTIALS_FILE, DeviceSigningIn, Server, assert_success, assert_usage, body,
    reject_device_login,
};

#[tokio::test]
async fn login_device_saves_cli_sign_in_when_person_approves() {
    let server = Server::configured(json!({"device_poll_interval_seconds": 1})).await;
    server.login().await;
    let previous = server.credentials();
    let marker = server.directory.path().join("browser-opened");
    let opener = server.directory.path().join("opener");
    fs::write(
        &opener,
        format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&opener, fs::Permissions::from_mode(0o700)).unwrap();
    let child = server
        .cli(&["login", &server.url, "--device", "--output", "json"])
        .env("BROWSER", opener)
        .spawn()
        .unwrap();
    let login = DeviceSigningIn::start(child).await;
    server.decide_device(&login, "approve").await;
    let output = login.finish().await;
    assert_success(&output);
    assert_eq!(body(&output), json!({"data": null, "meta": {}}));
    assert!(!marker.exists());
    let saved = server.credentials();
    assert_eq!(saved["server_url"], format!("{}/", server.url));
    assert!(saved["access_token"] != previous["access_token"]);
    assert_eq!(
        fs::metadata(server.home()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(server.home().join(CREDENTIALS_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    let later = server.run(&["agent", "list", "--output", "json"]).await;
    assert_success(&later);
    assert_eq!(body(&later), json!({"data": [], "meta": {}}));
    for result in [&output, &later] {
        for key in ["access_token", "refresh_token"] {
            assert!(
                !String::from_utf8_lossy(&result.stdout).contains(saved[key].as_str().unwrap())
            );
            assert!(
                !String::from_utf8_lossy(&result.stderr).contains(saved[key].as_str().unwrap())
            );
        }
    }
    let mut expired = saved.clone();
    expired["expires_at"] = json!(0);
    fs::write(server.home().join(CREDENTIALS_FILE), expired.to_string()).unwrap();
    assert_success(&server.run(&["agent", "list"]).await);
    assert!(server.credentials()["refresh_token"] != saved["refresh_token"]);
    assert_success(&server.run(&["logout"]).await);
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
}

#[tokio::test]
async fn login_device_fails_when_person_declines() {
    let server = Server::configured(json!({"device_poll_interval_seconds": 1})).await;
    let login = server.device_signing_in().await;
    server.decide_device(&login, "decline").await;
    assert_failure(&login.finish().await, "access_denied", "declined");
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
}

#[tokio::test]
async fn login_device_fails_when_request_expires() {
    let server = Server::configured(json!({"device_poll_interval_seconds": 1,
        "device_request_lifetime_seconds": 2}))
    .await;
    let login = server.device_signing_in().await;
    assert_failure(&login.finish().await, "expired_token", "expired");
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
}

#[tokio::test]
async fn login_device_preserves_cli_sign_in_when_new_sign_in_fails() {
    let server = Server::start().await;
    server.login().await;
    let before = fs::read(server.home().join(CREDENTIALS_FILE)).unwrap();
    for (code, message) in [("access_denied", "declined"), ("expired_token", "expired")] {
        assert_failure(&reject_device_login(&server, code).await, code, message);
        assert!(fs::read(server.home().join(CREDENTIALS_FILE)).unwrap() == before);
        assert_success(&server.run(&["agent", "list"]).await);
    }
}

#[tokio::test]
async fn login_device_fails_when_no_browser_is_also_given() {
    let server = Server::start().await;
    let output = server
        .run(&[
            "login",
            &server.url,
            "--device",
            "--no-browser",
            "--output",
            "json",
        ])
        .await;
    assert_usage(&output);
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
}

#[expect(
    clippy::unwrap_used,
    reason = "The executable scenario must return a complete error object."
)]
fn assert_failure(output: &Output, code: &str, message: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_str(
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(error["code"], code);
    assert!(error["message"].as_str().unwrap().contains(message));
}
