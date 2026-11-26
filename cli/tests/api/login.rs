use std::fs;
use std::os::unix::fs::PermissionsExt;

use reqwest::StatusCode;
use serde_json::{Value, json};
use url::Url;

use crate::helpers::CREDENTIALS_FILE;
use crate::helpers::{Server, SigningIn, WAIT, assert_success, assert_usage_lines, body, query};
use tokio::time::timeout;

#[tokio::test]
async fn login_saves_cli_sign_in_when_person_approves() {
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
async fn login_preserves_cli_sign_in_when_new_sign_in_fails() {
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
    assert_command_help_documented(&server).await;
    for arguments in [
        vec!["login"],
        vec!["task"],
        vec!["request"],
        vec!["unknown"],
        vec!["space", "create", "--unknown"],
        vec!["agent", "missing"],
        vec!["--output=json", "invalid"],
        vec!["agent", "list", "--page-size", "-1", "--output=json"],
        vec!["agent", "revoke"],
        vec!["file", "url", "SPACE-1"],
        vec!["file", "delete", "SPACE-1", "task-id"],
        vec!["task", "list"],
        vec!["task", "show", "SPACE-1"],
        vec!["task", "cancel", "SPACE-1"],
        vec!["task", "delete", "SPACE-1"],
        vec!["checkpoint", "delete", "SPACE-1", "task-id"],
        vec!["task", "scratchpad", "SPACE-1"],
    ] {
        let output = server.run(&arguments).await;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        if arguments.contains(&"--output=json") {
            let error: Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["code"], "cli.usage");
        } else {
            assert_usage_lines(&server, &arguments, &output).await;
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

const HELP_COMMANDS: &[&[&str]] = &[
    &[],
    &["--help"],
    &["agent", "--help"],
    &["agent", "list", "--help"],
    &["agent", "revoke", "--help"],
    &["file", "--help"],
    &["file", "delete", "--help"],
    &["file", "url", "--help"],
    &["checkpoint", "--help"],
    &["checkpoint", "delete", "--help"],
    &["knowledge", "--help"],
    &["knowledge", "change", "--help"],
    &["knowledge", "create", "--help"],
    &["knowledge", "delete", "--help"],
    &["knowledge", "deleted", "--help"],
    &["knowledge", "history", "--help"],
    &["knowledge", "list", "--help"],
    &["knowledge", "rollback", "--help"],
    &["knowledge", "show", "--help"],
    &["login", "--help"],
    &["logout", "--help"],
    &["member", "--help"],
    &["member", "add", "--help"],
    &["member", "list", "--help"],
    &["member", "remove", "--help"],
    &["member", "set-role", "--help"],
    &["request", "--help"],
    &["request", "approve", "--help"],
    &["request", "change", "--help"],
    &["request", "create", "--help"],
    &["request", "delete", "--help"],
    &["request", "list", "--help"],
    &["request", "reject", "--help"],
    &["request", "revise", "--help"],
    &["request", "show", "--help"],
    &["space", "--help"],
    &["space", "change", "--help"],
    &["space", "create", "--help"],
    &["space", "list", "--help"],
    &["space", "show", "--help"],
    &["task", "--help"],
    &["task", "cancel", "--help"],
    &["task", "delete", "--help"],
    &["task", "list", "--help"],
    &["task", "plan", "--help"],
    &["task", "results", "--help"],
    &["task", "scratchpad", "--help"],
    &["task", "show", "--help"],
    &["user", "--help"],
    &["user", "list", "--help"],
];

async fn assert_command_help_documented(server: &Server) {
    for arguments in HELP_COMMANDS {
        let output = server.run(arguments).await;
        assert_success(&output);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Usage:"));
        assert!(output.stderr.is_empty());
        assert_help_lines_described(&stdout, arguments);
        if arguments.starts_with(&["task", "delete"]) {
            assert!(stdout.contains("Delete a task, its checkpoints, files and their content"));
        }
    }
}

fn assert_help_lines_described(help: &str, arguments: &[&str]) {
    let mut section = "";
    let mut current: Option<(&str, String)> = None;
    for line in help.lines() {
        if matches!(line, "Arguments:" | "Options:" | "Commands:" | "Usage:") {
            flush_help_row(&mut current, arguments);
            section = line;
            continue;
        }
        if line.is_empty() {
            flush_help_row(&mut current, arguments);
            section = "";
            continue;
        }
        if !matches!(section, "Arguments:" | "Options:") {
            continue;
        }
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent <= 6 {
            flush_help_row(&mut current, arguments);
            let help_part = trimmed.split_once("  ").map_or("", |(_, rest)| rest.trim());
            current = Some((line, help_part.to_owned()));
        } else if let Some((_, text)) = &mut current {
            text.push(' ');
            text.push_str(trimmed);
        }
    }
    flush_help_row(&mut current, arguments);
}

fn flush_help_row(current: &mut Option<(&str, String)>, arguments: &[&str]) {
    let Some((row, text)) = current.take() else {
        return;
    };
    let description = strip_clap_annotations(&text);
    assert!(
        !description.trim().is_empty(),
        "missing description in {arguments:?} line: {row}"
    );
}

fn strip_clap_annotations(mut text: &str) -> String {
    let mut description = String::new();
    while let Some(start) = text.find('[') {
        description.push_str(&text[..start]);
        let rest = &text[start..];
        let is_annotation = ["[default:", "[possible values:", "[aliases:", "[env:"]
            .iter()
            .any(|prefix| rest.starts_with(prefix));
        if is_annotation && let Some(end) = rest.find(']') {
            text = &rest[end + 1..];
            continue;
        }
        description.push('[');
        text = &rest[1..];
    }
    description.push_str(text);
    description
}
