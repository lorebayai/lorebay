use std::fs;
use std::os::unix::fs::PermissionsExt;

use reqwest::StatusCode;
use serde_json::{Value, json};
use url::Url;

use crate::helpers::CREDENTIALS_FILE;
use crate::helpers::{Server, SigningIn, WAIT, assert_success, body, query};
use tokio::time::timeout;

#[tokio::test]
async fn login_saves_login_session_when_person_approves() {
    let server = Server::start().await;
    let output = server.login().await;
    assert_eq!(body(&output), json!({"data": null, "meta": {}}));
    let path = server.home().join(CREDENTIALS_FILE);
    assert_eq!(
        fs::metadata(server.home()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let credentials = server.credentials();
    assert_eq!(credentials["server_url"], format!("{}/", server.url));
    let later = server.run(&["agent", "list", "--output", "json"]).await;
    assert_success(&later);
    assert_eq!(body(&later), json!({"data": [], "meta": {}}));
    for output in [&output, &later] {
        for key in ["access_token", "refresh_token"] {
            let secret = credentials[key].as_str().unwrap();
            assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
            assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
        }
    }
}

#[tokio::test]
async fn login_fails_when_person_declines() {
    let server = Server::start().await;
    let login = server
        .signing_in(&["login", &server.url, "--no-browser", "--output", "json"])
        .await;
    server.decide(&login.link, "decline").await;
    let output = login.finish().await;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_str(
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(error["code"], "access_denied");
    assert!(error["message"].as_str().unwrap().contains("declined"));
    assert!(!server.home().join(CREDENTIALS_FILE).exists());
}

#[tokio::test]
async fn login_fails_when_server_is_plain_http_outside_loopback() {
    let server = Server::start().await;
    for address in [
        "http://example.com",
        "http://192.168.1.1",
        "http://[::2]",
        "ftp://localhost",
        "invalid",
    ] {
        let output = server
            .run(&["login", address, "--no-browser", "--output", "json"])
            .await;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["code"], "cli.usage");
        assert!(!server.home().join(CREDENTIALS_FILE).exists());
    }
}

#[tokio::test]
async fn login_ignores_requests_when_callback_does_not_match() {
    let server = Server::start().await;
    let login = server
        .signing_in(&["login", &server.url, "--no-browser"])
        .await;
    let redirect = query(&login.link, "redirect_uri");
    let state = query(&login.link, "state");
    for (path, state, issuer) in [
        ("/favicon.ico", state.as_str(), server.url.as_str()),
        ("/callback", "wrong", server.url.as_str()),
        ("/callback", state.as_str(), "https://elsewhere.invalid"),
        ("/callback", state.as_str(), ""),
    ] {
        let mut stray = Url::parse(&redirect).unwrap();
        stray.set_path(path);
        stray
            .query_pairs_mut()
            .append_pair("state", state)
            .append_pair("iss", issuer)
            .append_pair("code", "stray-code");
        assert_eq!(
            server.client.get(stray).send().await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }
    server.decide(&login.link, "approve").await;
    let output = login.finish().await;
    assert_success(&output);
    assert!(!String::from_utf8_lossy(&output.stderr).contains("stray-code"));
    assert_success(&server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn login_preserves_login_session_when_new_sign_in_fails() {
    let server = Server::start().await;
    server.login().await;
    let before = fs::read(server.home().join(CREDENTIALS_FILE)).unwrap();
    let login = server
        .signing_in(&["login", &server.url, "--no-browser"])
        .await;
    server.decide(&login.link, "decline").await;
    assert_eq!(login.finish().await.status.code(), Some(1));
    assert!(before == fs::read(server.home().join(CREDENTIALS_FILE)).unwrap());
    assert_success(&server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn login_continues_when_browser_cannot_open() {
    let server = Server::start().await;
    let login = server.signing_in(&["login", &server.url]).await;
    server.decide(&login.link, "approve").await;
    let output = login.finish().await;
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stderr).contains("Open the printed link"));
}

#[tokio::test]
async fn help_and_usage_use_documented_exit_codes() {
    let server = Server::start().await;
    for arguments in [
        vec![],
        vec!["--help"],
        vec!["login", "--help"],
        vec!["agent", "list", "--help"],
        vec!["task", "--help"],
        vec!["task", "list", "--help"],
        vec!["task", "show", "--help"],
        vec!["task", "close", "--help"],
        vec!["task", "delete", "--help"],
        vec!["note", "delete", "--help"],
        vec!["task", "scratchpad", "--help"],
    ] {
        let output = server.run(&arguments).await;
        assert_success(&output);
        assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
        assert!(output.stderr.is_empty());
    }
    for arguments in [
        vec!["login"],
        vec!["agent", "missing"],
        vec!["--output=json", "invalid"],
        vec!["agent", "list", "--page-size", "-1", "--output=json"],
        vec!["agent", "revoke"],
        vec!["task", "list"],
        vec!["task", "show", "SPACE-1"],
        vec!["task", "close", "SPACE-1"],
        vec!["task", "delete", "SPACE-1"],
        vec!["note", "delete", "SPACE-1", "task-id"],
        vec!["task", "scratchpad", "SPACE-1"],
    ] {
        let output = server.run(&arguments).await;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        if arguments.contains(&"--output=json") {
            let error: Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["code"], "cli.usage");
        } else {
            assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
        }
    }
    assert!(!server.home().exists());
}

#[tokio::test]
async fn login_opens_browser_unless_no_browser_is_given() {
    let server = Server::start().await;
    let opener = server.directory.path().join("browser");
    fs::write(&opener, "#!/bin/sh\nprintf '%s' \"$1\" > opened-url\n").unwrap();
    fs::set_permissions(&opener, fs::Permissions::from_mode(0o700)).unwrap();
    for no_browser in [false, true] {
        let mut arguments = vec!["login", &server.url];
        if no_browser {
            arguments.push("--no-browser");
        }
        let child = server
            .cli(&arguments)
            .env("BROWSER", &opener)
            .spawn()
            .unwrap();
        let login = SigningIn::start(child).await;
        server.decide(&login.link, "approve").await;
        let expected_link = login.link.to_string();
        assert_success(&login.finish().await);
        let opened = server.directory.path().join("opened-url");
        if no_browser {
            assert!(!opened.exists());
        } else {
            assert_eq!(fs::read_to_string(&opened).unwrap(), expected_link);
            fs::remove_file(opened).unwrap();
        }
    }
}

#[tokio::test]
async fn login_uses_home_directory_when_override_is_absent() {
    let server = Server::start().await;
    let child = server
        .cli(&["login", &server.url, "--no-browser"])
        .env_remove("LOREBAY_HOME")
        .spawn()
        .unwrap();
    let login = SigningIn::start(child).await;
    server.decide(&login.link, "approve").await;
    assert_success(&login.finish().await);
    let path = server
        .directory
        .path()
        .join("home/.lorebay")
        .join(CREDENTIALS_FILE);
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(server.directory.path().join("home/.lorebay"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(!server.home().exists());
    let output = timeout(
        WAIT,
        server
            .cli(&["agent", "list"])
            .env_remove("LOREBAY_HOME")
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_success(&output);
}

#[tokio::test]
async fn login_preserves_permissions_when_credentials_directory_exists() {
    let server = Server::start().await;
    fs::create_dir(server.home()).unwrap();
    fs::set_permissions(server.home(), fs::Permissions::from_mode(0o750)).unwrap();
    server.login().await;
    assert_eq!(
        fs::metadata(server.home()).unwrap().permissions().mode() & 0o777,
        0o750
    );
    assert_eq!(
        fs::metadata(server.home().join(CREDENTIALS_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_success(&server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn login_completes_while_browser_launcher_waits() {
    let server = Server::start().await;
    let opener = server.directory.path().join("lynx");
    fs::write(&opener, "#!/bin/sh\nexec /bin/sleep 5\n").unwrap();
    fs::set_permissions(&opener, fs::Permissions::from_mode(0o700)).unwrap();
    let child = server
        .cli(&["login", &server.url])
        .env("BROWSER", &opener)
        .spawn()
        .unwrap();
    let login = SigningIn::start(child).await;
    server.decide(&login.link, "approve").await;
    assert_success(&login.finish().await);
    assert_success(&server.run(&["agent", "list"]).await);
}
