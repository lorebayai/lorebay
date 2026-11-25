use std::fs;
use std::path::Path;
use std::process::Output;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use super::{
    SECOND_USERNAME, Server, assert_success, assert_usage, body, content_input, create_entry,
    knowledge_json, run_list_hint,
};

pub async fn contributor(server: &Server, space: &str) {
    server.login_as(SECOND_USERNAME).await;
    assert_success(
        &server
            .run(&[
                "member",
                "add",
                space,
                SECOND_USERNAME,
                "--role",
                "contributor",
            ])
            .await,
    );
}

pub async fn person_request_json(server: &Server, arguments: &[&str]) -> Value {
    let mut command = vec!["request"];
    command.extend_from_slice(arguments);
    if matches!(arguments[0], "approve" | "reject") && !command.contains(&"--revision") {
        command.extend(["--revision", "1"]);
    }
    command.extend(["--output", "json"]);
    let output = server.run(&command).await;
    assert_success(&output);
    assert!(output.stderr.is_empty());
    assert_eq!(body(&output)["meta"], json!({}));
    body(&output)["data"].clone()
}

pub async fn create_person_request(
    server: &Server,
    space: &str,
    name: &str,
    summary: &str,
    content: &str,
) -> Value {
    person_request_json(
        server,
        &[
            "create",
            space,
            name,
            "--summary",
            summary,
            "--content",
            content,
        ],
    )
    .await
}

pub async fn show_person_request(server: &Server, space: &str, key: &str) -> Value {
    let person_request = person_request_json(server, &["show", space, key]).await;
    let api = server
        .json(
            Method::GET,
            &format!("/api/v1/spaces/{space}/requests/{key}"),
            Value::Null,
            server.credentials()["access_token"].as_str(),
        )
        .await;
    assert_eq!(api, json!({"data": person_request, "meta": {}}));
    person_request
}

pub async fn submit_agent_person_request(
    server: &Server,
    space: &str,
    agent: &Value,
    task: &Value,
) -> Value {
    let task_id = task["task_id"].as_str().unwrap();
    server
        .json(
            Method::POST,
            &format!("/api/v1/spaces/{space}/tasks/{task_id}/requests"),
            json!({
                "action": "create", "name": "agent.md", "summary": "Agent guide",
                "content": "# Shared\n\tcontent",
            }),
            agent["access_token"].as_str(),
        )
        .await["data"]
        .clone()
}

pub fn assert_person_request_fields(output: &Output, person_request: &Value, target: &str) {
    assert_success(output);
    assert!(output.stderr.is_empty());
    let mut expected = String::new();
    for (label, field) in [
        ("Key", "key"),
        ("Action", "action"),
        ("Status", "status"),
        ("Target", "target"),
        ("Proposed name", "new_name"),
        ("Proposed summary", "new_summary"),
        ("Content", "new_content"),
        ("Requested by", "requested_by"),
        ("Submitting agent", "requester_agent_name"),
        ("Source task", "from_task_key"),
        ("Created at", "created_at"),
        ("Base version", "base_version"),
        ("Base name", "base_name"),
        ("Revision", "revision"),
        ("Outdated", "outdated"),
        ("Last revising agent", "reviser_agent_name"),
        ("Revised by", "revised_by"),
        ("Revised at", "revised_at"),
        ("Decided by", "decided_by"),
        ("Decided at", "decided_at"),
        ("Rejection reason", "reject_reason"),
    ] {
        let value = match (field, &person_request[field]) {
            ("target", _) => target.to_owned(),
            (_, Value::Null) => String::new(),
            (_, Value::String(text)) => text.clone(),
            (_, value) => value.to_string(),
        };
        let escaped: String = value
            .chars()
            .flat_map(|character| {
                if (character.is_control() || character == '\u{202e}')
                    && !(field == "new_content" && matches!(character, '\n' | '\t'))
                {
                    character.escape_default().collect::<Vec<_>>()
                } else {
                    vec![character]
                }
            })
            .collect();
        expected.push_str(&format!("{label}: {escaped}\n"));
    }
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}

pub async fn assert_person_request_pages(
    server: &Server,
    command: &[&str],
    expected: &[(&str, &str)],
) {
    for mode in ["human", "json"] {
        let mut arguments = command.to_vec();
        arguments.extend(["--page-size", "1", "--output", mode]);
        let mut output = server.run(&arguments).await;
        for (index, (key, target)) in expected.iter().enumerate() {
            assert_success(&output);
            let person_request = show_person_request(server, command[2], key).await;
            if mode == "json" {
                let row = &body(&output)["data"][0];
                for field in [
                    "key",
                    "action",
                    "status",
                    "requested_by",
                    "requester_agent_name",
                    "target",
                    "new_name",
                    "new_summary",
                    "created_at",
                ] {
                    assert_eq!(row[field], person_request[field]);
                }
                assert_eq!(body(&output)["data"].as_array().unwrap().len(), 1);
            } else {
                let text = String::from_utf8_lossy(&output.stdout);
                assert_eq!(text.lines().count(), 2);
                assert!(
                    text.starts_with("Key\tAction\tStatus\tOutdated\tTarget\tProposed summary\n")
                );
                assert_eq!(text.lines().nth(1).unwrap().split('\t').next(), Some(*key));
                assert_eq!(
                    text.lines().nth(1).unwrap().split('\t').nth(4),
                    Some(*target)
                );
            }
            let hint = String::from_utf8(output.stderr.clone()).unwrap();
            assert_eq!(hint.is_empty(), index + 1 == expected.len());
            if hint.is_empty() {
                if mode == "json" {
                    assert_eq!(body(&output)["meta"], json!({}));
                }
                break;
            }
            assert!(hint.starts_with(&format!("Next page: lorebay request list '{}'", command[2])));
            assert!(hint.contains(" --page-size 1 --cursor '"));
            assert!(hint.ends_with(&format!(" --output {mode}\n")));
            for pair in command[3..].as_chunks::<2>().0 {
                let escaped = pair[1].replace('\'', "'\\''");
                assert!(hint.contains(&format!(" {} '{escaped}'", pair[0])));
            }
            if mode == "json" {
                assert!(hint.contains(body(&output)["meta"]["next_cursor"].as_str().unwrap()));
            }
            output = run_list_hint(server, &hint).await;
        }
    }
}

pub async fn assert_person_request_error(
    server: &Server,
    output: &Output,
    request: (&str, &str, &str, Value),
    mode: &str,
) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
    let problem = person_request_problem(server, request).await;
    if mode == "json" {
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stderr).unwrap(),
            problem
        );
    } else {
        let mut expected = problem["title"].as_str().unwrap().to_owned();
        if let Some(errors) = problem["errors"].as_array() {
            for error in errors {
                expected.push_str(&format!(": {}", error["message"].as_str().unwrap()));
            }
        }
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!("{expected}\n")
        );
    }
}

pub async fn assert_person_request_usage(server: &mut Server, space: &str) {
    server.stop();
    for mut command in [
        vec!["list", space, "--status", "decided"],
        vec!["change", space, "KN-1"],
        vec!["create", space, "missing.md", "--summary", "Summary"],
        vec![
            "create",
            space,
            "both.md",
            "--summary",
            "Summary",
            "--content",
            "Text",
            "--content-file",
            "-",
        ],
        vec![
            "change",
            space,
            "KN-1",
            "--content",
            "Text",
            "--content-file",
            "-",
        ],
        vec!["reject", space, "RQ-1"],
    ] {
        command.insert(0, "request");
        command.extend(["--output", "json"]);
        assert_usage(&server.run(&command).await);
    }
}

pub async fn assert_person_request_sources(server: &Server, space: &str, path: &Path) {
    create_entry(server, space, "guide.md", "Keep summary", "Original").await;
    fs::write(path, b"File change\r\n\tcontent\n").unwrap();
    let changed = person_request_json(
        server,
        &[
            "change",
            space,
            "KN-1",
            "--content-file",
            path.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(changed["new_content"], "File change\r\n\tcontent\n");
    assert_eq!(changed["new_summary"], Value::Null);
    for arguments in [
        vec![
            "request",
            "create",
            space,
            "stdin.md",
            "--summary",
            "Input",
            "--content-file",
            "-",
            "--output",
            "json",
        ],
        vec![
            "request",
            "change",
            space,
            "guide.md",
            "--content-file",
            "-",
            "--output",
            "json",
        ],
    ] {
        let output = content_input(server, &arguments, b"stdin\r\n\tcontent\n").await;
        assert_success(&output);
        assert!(output.stderr.is_empty());
        let person_request = body(&output)["data"].clone();
        assert_eq!(person_request["new_content"], "stdin\r\n\tcontent\n");
        assert_eq!(
            show_person_request(server, space, person_request["key"].as_str().unwrap()).await,
            person_request
        );
        assert_usage(&content_input(server, &arguments, &[0xff]).await);
    }
    assert_person_request_file_limits(server, space, path).await;
    assert_invalid_person_request_files(server, space, path).await;
    assert_eq!(
        knowledge_json(server, &["show", space, "KN-1"]).await["content"],
        "Original"
    );
}

pub async fn assert_empty_person_request_search(server: &Server, space: &str) {
    for mode in ["human", "json"] {
        let empty = server
            .run(&[
                "request",
                "list",
                space,
                "--keyword",
                "absent",
                "--output",
                mode,
            ])
            .await;
        assert_success(&empty);
        assert!(empty.stderr.is_empty());
        if mode == "json" {
            assert_eq!(body(&empty), json!({"data": [], "meta": {}}));
        } else {
            assert_eq!(
                empty.stdout,
                b"Key\tAction\tStatus\tOutdated\tTarget\tProposed summary\n"
            );
        }
    }
}

pub async fn propose_rollback(server: &Server, space: &str) -> (Value, Value) {
    let old = server
        .run_as(
            SECOND_USERNAME,
            &[
                "knowledge",
                "show",
                space,
                "KN-1",
                "--version",
                "1",
                "--output",
                "json",
            ],
        )
        .await;
    assert_success(&old);
    let old = body(&old)["data"].clone();
    let output = server
        .run_as(
            SECOND_USERNAME,
            &[
                "request",
                "change",
                space,
                "KN-1",
                "--summary",
                old["summary"].as_str().unwrap(),
                "--content",
                old["content"].as_str().unwrap(),
                "--output",
                "json",
            ],
        )
        .await;
    assert_success(&output);
    assert!(output.stderr.is_empty());
    let person_request = body(&output)["data"].clone();
    assert_eq!(
        show_person_request(server, space, "RQ-1").await,
        person_request
    );
    (old, person_request)
}

async fn assert_person_request_file_limits(server: &Server, space: &str, path: &Path) {
    for bytes in [
        vec![0; 1024 * 1024],
        vec![b'x'; 1024 * 1024 + 1],
        Vec::new(),
    ] {
        fs::write(path, &bytes).unwrap();
        for mut arguments in [
            vec!["create", space, "limit.md", "--summary", "Limit"],
            vec!["change", space, "KN-1"],
        ] {
            arguments.insert(0, "request");
            arguments.extend(["--content-file", path.to_str().unwrap(), "--output", "json"]);
            let output = server.run(&arguments).await;
            if bytes.len() == 1024 * 1024 {
                assert_success(&output);
                let person_request = body(&output)["data"].clone();
                assert_eq!(
                    person_request["new_content"].as_str().unwrap().as_bytes(),
                    bytes
                );
                assert_eq!(
                    show_person_request(server, space, person_request["key"].as_str().unwrap())
                        .await,
                    person_request
                );
            } else {
                assert_eq!(output.status.code(), Some(1));
                assert!(output.stdout.is_empty());
                let error: Value = serde_json::from_slice(&output.stderr).unwrap();
                assert_eq!(error["code"], "request.invalid_body");
            }
        }
    }
}

async fn person_request_problem(server: &Server, request: (&str, &str, &str, Value)) -> Value {
    let (space, key, action, input) = request;
    let credentials = if action == "approve" {
        serde_json::from_slice::<Value>(
            &fs::read(
                server
                    .directory
                    .path()
                    .join(SECOND_USERNAME)
                    .join("credentials.json"),
            )
            .unwrap(),
        )
        .unwrap()
    } else {
        server.credentials()
    };
    let response = server
        .client
        .post(format!(
            "{}/api/v1/spaces/{space}/requests/{key}/{action}",
            server.url
        ))
        .bearer_auth(credentials["access_token"].as_str().unwrap())
        .json(&input)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        if action == "approve" {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::BAD_REQUEST
        }
    );
    let problem: Value = response.json().await.unwrap();
    assert_eq!(
        problem["code"],
        if action == "approve" {
            "person_request.own_party"
        } else {
            "request.invalid_body"
        }
    );
    problem
}

async fn assert_invalid_person_request_files(server: &Server, space: &str, path: &Path) {
    fs::write(path, [0xff, 0xfe]).unwrap();
    for source in [
        path.to_path_buf(),
        server.directory.path().join("missing.md"),
        server.directory.path().to_path_buf(),
    ] {
        for mut arguments in [
            vec!["create", space, "invalid.md", "--summary", "Summary"],
            vec!["change", space, "KN-1"],
        ] {
            arguments.insert(0, "request");
            arguments.extend([
                "--content-file",
                source.to_str().unwrap(),
                "--output",
                "json",
            ]);
            assert_usage(&server.run(&arguments).await);
        }
    }
}

pub async fn assert_withdrawn_request(
    server: &Server,
    space: &str,
    key: &str,
    person_request: &Value,
) {
    let shown = server.run(&["request", "show", space, key]).await;
    assert_person_request_fields(&shown, person_request, "guide.md");
    assert!(
        String::from_utf8(shown.stdout)
            .unwrap()
            .contains("Rejection reason: Superseded\\n\\t\\u{1b}[31m\n")
    );
}
