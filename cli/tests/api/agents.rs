use std::process::Stdio;

use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::time::timeout;

use crate::helpers::{Server, WAIT, assert_success, body, human_time};

#[tokio::test]
async fn agent_list_returns_next_page_when_cursor_is_given() {
    let server = Server::start().await;
    server.login().await;
    for name in ["Alpha", "Beta", "Gamma"] {
        server.agent(name).await;
    }
    for mode in ["human", "json"] {
        let first = server
            .run(&["agent", "list", "--page-size", "2", "--output", mode])
            .await;
        assert_success(&first);
        let hint = String::from_utf8(first.stderr.clone()).unwrap();
        assert!(hint.starts_with("Next page: lorebay agent list --page-size 2 --cursor '"));
        assert!(hint.contains(&format!("--output {mode}")));
        let cursor = hint.split('\'').nth(1).unwrap();
        let last = server
            .run(&[
                "agent",
                "list",
                "--page-size",
                "2",
                "--cursor",
                cursor,
                "--output",
                mode,
            ])
            .await;
        assert_success(&last);
        assert!(last.stderr.is_empty());
        if mode == "json" {
            assert_eq!(body(&first)["meta"]["next_cursor"], cursor);
            assert_eq!(body(&first)["data"].as_array().unwrap().len(), 2);
            assert_eq!(body(&last)["data"][0]["name"], "Gamma");
            assert_eq!(body(&last)["data"].as_array().unwrap().len(), 1);
            assert_eq!(body(&last)["meta"], json!({}));
        } else {
            let text = String::from_utf8(first.stdout).unwrap();
            assert_eq!(text.lines().count(), 3);
            assert!(text.contains("Alpha") && text.contains("Beta") && !text.contains("Gamma"));
            assert!(String::from_utf8_lossy(&last.stdout).contains("Gamma"));
        }
    }
}

#[tokio::test]
async fn agent_revoke_removes_agent_from_list() {
    let server = Server::start().await;
    server.login().await;
    let name = "Claude Code / home? #+%";
    server.agent(name).await;
    let before = server.run(&["agent", "list", "--output", "json"]).await;
    assert_success(&before);
    assert_eq!(body(&before)["data"][0]["name"], name);
    let output = server
        .run(&["agent", "revoke", name, "--yes", "--output", "json"])
        .await;
    assert_success(&output);
    assert_eq!(body(&output), json!({"data": null, "meta": {}}));
    let after = server.run(&["agent", "list", "--output", "json"]).await;
    assert_success(&after);
    assert_eq!(body(&after)["data"], json!([]));
}

#[tokio::test]
async fn agent_revoke_fails_with_server_error_when_agent_is_unknown() {
    let server = Server::start().await;
    server.login().await;
    for mode in ["human", "json"] {
        let output = server
            .run(&["agent", "revoke", "Unknown", "--yes", "--output", mode])
            .await;
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
        if mode == "json" {
            let error: Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(
                error,
                json!({"type": "about:blank", "title": "Not Found", "status": 404,
                "code": "http.not_found", "errors": []})
            );
        } else {
            assert_eq!(output.stderr, b"Not Found\n");
        }
    }
}

#[tokio::test]
async fn agent_revoke_cancels_when_input_is_not_terminal() {
    let server = Server::start().await;
    server.login().await;
    server.agent("Keep me").await;
    let mut command = server.cli(&["agent", "revoke", "Keep me", "--output", "json"]);
    let mut child = command.stdin(Stdio::piped()).spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(b"yes\n").await;
    let output = timeout(WAIT, child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_str(
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .last()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(error["code"], "cli.cancelled");
    let list = server.run(&["agent", "list", "--output", "json"]).await;
    assert_success(&list);
    assert_eq!(body(&list)["data"][0]["name"], "Keep me");
}

#[tokio::test]
async fn agent_list_escapes_control_characters_in_server_text() {
    let server = Server::start().await;
    server.login().await;
    let name = "Agent\u{1b}[31m\n\t\r\u{7}";
    server.agent(name).await;
    let saved = server.run(&["agent", "list", "--output", "json"]).await;
    assert_success(&saved);
    let created = body(&saved)["data"][0]["created_at"].clone();
    assert!(created.is_i64());
    let human = server.run(&["agent", "list"]).await;
    assert_success(&human);
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(text.contains("Agent\\u{1b}[31m\\n\\t\\r\\u{7}"));
    assert!(text.contains(&human_time(&created)));
    assert_eq!(text.lines().count(), 2);
    let json = server.run(&["agent", "list", "--output", "json"]).await;
    assert_success(&json);
    assert_eq!(body(&json)["data"][0]["name"], name);
    let cancelled = server.run(&["agent", "revoke", name]).await;
    assert_eq!(cancelled.status.code(), Some(1));
    let stderr = String::from_utf8(cancelled.stderr).unwrap();
    assert!(!stderr.contains('\u{1b}'));
    assert_eq!(stderr.lines().count(), 2);
}

#[tokio::test]
async fn agent_list_preserves_field_errors_when_page_is_invalid() {
    let server = Server::start().await;
    server.login().await;
    for mode in ["human", "json"] {
        let output = server
            .run(&["agent", "list", "--page-size", "0", "--output", mode])
            .await;
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
        if mode == "json" {
            let error: Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["code"], "request.invalid_query");
            assert_eq!(error["errors"][0]["field"], "page_size");
        } else {
            assert!(String::from_utf8_lossy(&output.stderr).starts_with("Bad Request: "));
        }
    }
}
