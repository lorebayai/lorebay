use std::process::{Output, Stdio};

use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tokio::time::timeout;

use super::{Server, WAIT, assert_success, assert_task_error, body};

pub fn assert_cancelled(output: &Output, mode: &str, target: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(stderr.starts_with("Delete "));
    assert!(stderr.contains(target));
    assert!(stderr.contains("? [y/N]\n"));
    assert_eq!(stderr.lines().count(), 2);
    if mode == "json" {
        let error: Value = serde_json::from_str(stderr.lines().last().unwrap()).unwrap();
        assert_eq!(error["code"], "cli.cancelled");
    } else {
        assert!(stderr.ends_with("Operation cancelled.\n"));
    }
}

pub fn assert_deletion(output: &Output, mode: &str, message: &str) {
    assert_success(output);
    assert!(output.stderr.is_empty());
    if mode == "json" {
        assert_eq!(body(output), json!({"data": null, "meta": {}}));
    } else {
        assert_eq!(output.stdout, format!("{message}\n").as_bytes());
    }
}

pub async fn piped_yes(server: &Server, arguments: &[&str]) -> Output {
    let mut child = server.cli(arguments).stdin(Stdio::piped()).spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(b"yes\n").await;
    timeout(WAIT, child.wait_with_output())
        .await
        .unwrap()
        .unwrap()
}

pub async fn notes_output(server: &Server, space: &str, key: &str, mode: &str) -> Output {
    let output = server
        .run(&["task", "scratchpad", space, key, "--output", mode])
        .await;
    assert_success(&output);
    assert!(output.stderr.is_empty());
    output
}

pub fn printed_note_time(output: &Output, mode: &str) -> String {
    if mode == "json" {
        assert!(body(output)["data"][0]["time"].is_i64());
        return body(output)["data"][0]["time"].to_string();
    }
    String::from_utf8(output.stdout.clone())
        .unwrap()
        .lines()
        .nth(1)
        .unwrap()
        .split('\t')
        .next()
        .unwrap()
        .to_owned()
}

pub async fn assert_task_absent(server: &Server, space: &str, key: &str) {
    for mut arguments in [
        vec!["task", "show", space, key],
        vec!["task", "scratchpad", space, key],
    ] {
        arguments.extend(["--output", "json"]);
        assert_task_error(&server.run(&arguments).await, "json", "task.not_found");
    }
}
