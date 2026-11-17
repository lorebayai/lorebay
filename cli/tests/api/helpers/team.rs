use std::process::Output;

use serde_json::{Value, json};

use super::{SECOND_USERNAME, Server, assert_success, body};

pub async fn team() -> Server {
    let server = Server::start().await;
    server.login().await;
    server.login_as(SECOND_USERNAME).await;
    server
}

pub async fn create(server: &Server, name: &str, summary: &str) -> String {
    let output = server
        .run(&[
            "space",
            "create",
            "--name",
            name,
            "--summary",
            summary,
            "--output",
            "json",
        ])
        .await;
    assert_success(&output);
    let body = body(&output);
    assert_eq!(body["data"]["name"], name);
    assert_eq!(body["data"]["summary"], summary);
    assert_eq!(body["data"]["role"], "owner");
    assert_eq!(body["meta"], json!({}));
    body["data"]["key"].as_str().unwrap().to_owned()
}

pub fn assert_participant(output: &Output, mode: &str, role: &str) {
    assert_success(output);
    assert!(output.stderr.is_empty());
    if mode == "json" {
        assert_eq!(
            body(output),
            json!({"data": {"type": "human", "username": SECOND_USERNAME, "role": role}, "meta": {}})
        );
    } else {
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!("Type\tUsername\tAgent name\tRole\nhuman\t{SECOND_USERNAME}\t\t{role}\n")
        );
    }
}

pub async fn pages(
    server: &Server,
    command: &[&str],
    field: &str,
    expected: &[&str],
) -> Vec<Value> {
    let mut items = Vec::new();
    let expected_len = expected.len();
    for mode in ["human", "json"] {
        let mut cursor = String::new();
        for (index, expected) in expected.iter().enumerate() {
            let mut arguments = command.to_vec();
            arguments.extend(["--page-size", "1", "--output", mode]);
            if !cursor.is_empty() {
                arguments.extend(["--cursor", &cursor]);
            }
            let output = server.run(&arguments).await;
            assert_success(&output);
            if mode == "json" {
                assert_eq!(body(&output)["data"].as_array().unwrap().len(), 1);
                assert_eq!(body(&output)["data"][0][field], *expected);
                items.push(body(&output)["data"][0].clone());
            } else {
                assert_eq!(String::from_utf8_lossy(&output.stdout).lines().count(), 2);
                assert!(String::from_utf8_lossy(&output.stdout).contains(expected));
            }
            let hint = String::from_utf8(output.stderr.clone()).unwrap();
            assert_eq!(hint.is_empty(), index + 1 == expected_len);
            if hint.is_empty() {
                assert_eq!(index + 1, expected_len);
                if mode == "json" {
                    assert_eq!(body(&output)["meta"], json!({}));
                }
                break;
            }
            assert!(hint.starts_with(&format!("Next page: lorebay {} {}", command[0], command[1])));
            if command.len() == 3 {
                assert!(hint.contains(&format!("'{}'", command[2])));
            }
            assert!(hint.contains(" --page-size 1 --cursor '"));
            assert!(hint.contains(&format!("--output {mode}")));
            cursor = hint
                .split(" --cursor '")
                .nth(1)
                .unwrap()
                .split('\'')
                .next()
                .unwrap()
                .to_owned();
            if mode == "json" {
                assert_eq!(body(&output)["meta"]["next_cursor"], cursor);
            }
        }
    }
    items
}

pub async fn assert_removed(server: &Server, space: &str) {
    let roster = server
        .run(&["participant", "list", space, "--output", "json"])
        .await;
    assert_success(&roster);
    assert_eq!(
        body(&roster)["data"],
        json!([{"type": "human", "username": server.username, "role": "owner"}])
    );
    let spaces = server
        .run_as(SECOND_USERNAME, &["space", "list", "--output", "json"])
        .await;
    assert_success(&spaces);
    assert_eq!(body(&spaces), json!({"data": [], "meta": {}}));
}

pub fn assert_usage(output: &Output) {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["code"], "cli.usage");
}

pub fn assert_removal(removed: &Output, mode: &str) {
    assert_success(removed);
    assert!(removed.stderr.is_empty());
    if mode == "json" {
        assert_eq!(body(removed), json!({"data": null, "meta": {}}));
    } else {
        assert_eq!(removed.stdout, b"Participant removed.\n");
    }
}

pub fn path_commands(segment: &str) -> Vec<Vec<&str>> {
    vec![
        vec!["space", "show", segment],
        vec!["space", "change", segment, "--name", "Unchanged"],
        vec!["participant", "list", segment],
        vec![
            "participant",
            "add",
            segment,
            SECOND_USERNAME,
            "--role",
            "viewer",
        ],
        vec![
            "participant",
            "set-role",
            segment,
            SECOND_USERNAME,
            "--role",
            "viewer",
        ],
        vec!["participant", "remove", segment, SECOND_USERNAME],
        vec!["participant", "add", "SPACE-1", segment, "--role", "viewer"],
        vec![
            "participant",
            "set-role",
            "SPACE-1",
            segment,
            "--role",
            "viewer",
        ],
        vec!["participant", "remove", "SPACE-1", segment],
        vec!["agent", "revoke", segment, "--yes"],
        vec!["agent", "revoke", segment],
    ]
}
