use serde_json::{Value, json};

use super::{
    SECOND_USERNAME, Server, assert_knowledge_error, assert_success, assert_usage, body,
    run_list_hint, show_entry,
};

pub async fn knowledge_json(server: &Server, arguments: &[&str]) -> Value {
    let mut arguments = [vec!["knowledge"], arguments.to_vec()].concat();
    arguments.extend(["--output", "json"]);
    let output = server.run(&arguments).await;
    assert_success(&output);
    assert!(output.stderr.is_empty());
    assert_eq!(body(&output)["meta"], json!({}));
    body(&output)["data"].clone()
}

pub async fn read_entry_version(server: &Server, space: &str, entry: &str, version: i64) -> Value {
    knowledge_json(
        server,
        &["show", space, entry, "--version", &version.to_string()],
    )
    .await
}

pub async fn assert_history_pages(server: &Server, space: &str, entry: &str, versions: &[Value]) {
    let expected: Vec<_> = versions.iter().rev().map(|version| json!({
        "version": version["version"], "kind": version["kind"],
        "applied_by": version["applied_by"], "applied_at": version["applied_at"], "proposal_key": null,
    })).collect();
    for mode in ["human", "json"] {
        let mut hint = String::new();
        for (index, chunk) in expected.chunks(2).enumerate() {
            let output = if index == 0 {
                server
                    .run(&[
                        "knowledge",
                        "history",
                        space,
                        entry,
                        "--page-size",
                        "2",
                        "--output",
                        mode,
                    ])
                    .await
            } else {
                run_list_hint(server, &hint).await
            };
            assert_success(&output);
            if mode == "json" {
                assert_eq!(body(&output)["data"], json!(chunk));
            } else {
                let mut text = "Version\tKind\tApplied by\tApplied at\tProposal\n".to_owned();
                for row in chunk {
                    text.push_str(&format!(
                        "{}\t{}\t{}\t{}\t\n",
                        row["version"],
                        row["kind"].as_str().unwrap(),
                        row["applied_by"].as_str().unwrap(),
                        row["applied_at"]
                    ));
                }
                assert_eq!(String::from_utf8(output.stdout.clone()).unwrap(), text);
            }
            hint = String::from_utf8(output.stderr.clone()).unwrap();
            assert_eq!(hint.is_empty(), index + 1 == expected.len().div_ceil(2));
            if !hint.is_empty() {
                assert!(
                    hint.starts_with(&format!("Next page: lorebay knowledge history '{space}' "))
                );
                assert!(hint.contains(" --page-size 2 --cursor '"));
                assert!(hint.ends_with(&format!(" --output {mode}\n")));
                if mode == "json" {
                    assert!(hint.contains(body(&output)["meta"]["next_cursor"].as_str().unwrap()));
                }
            } else if mode == "json" {
                assert_eq!(body(&output)["meta"], json!({}));
            }
        }
    }
}

pub async fn assert_history_errors(server: &Server, space: &str) {
    for mode in ["human", "json"] {
        for (mut arguments, code) in [
            (
                vec!["knowledge", "show", space, "KN-1", "--version", "99"],
                "knowledge.version_not_found",
            ),
            (
                vec!["knowledge", "show", space, "KN-1", "--version", "-1"],
                "knowledge.version_not_found",
            ),
            (
                vec!["knowledge", "history", space, "missing.md"],
                "knowledge.not_found",
            ),
            (
                vec!["knowledge", "history", space, "KN-1", "--cursor", "invalid"],
                "request.invalid_query",
            ),
            (
                vec!["knowledge", "deleted", space, "--page-size", "0"],
                "request.invalid_query",
            ),
            (
                vec!["knowledge", "deleted", space, "--cursor", "invalid"],
                "request.invalid_query",
            ),
        ] {
            arguments.extend(["--output", mode]);
            assert_knowledge_error(&server.run(&arguments).await, mode, code);
        }
    }
}

pub async fn assert_rollback_refusals(server: &Server, space: &str, deleted: &Value) {
    for mode in ["human", "json"] {
        for version in ["3", "99", "0", "-1"] {
            let output = server
                .run(&[
                    "knowledge",
                    "rollback",
                    space,
                    "KN-1",
                    version,
                    "--output",
                    mode,
                ])
                .await;
            assert_knowledge_error(&output, mode, "request.invalid_body");
            assert_eq!(&show_entry(server, space, "KN-1").await, deleted);
        }
    }
}

pub async fn assert_viewer_reads_history(server: &Server, space: &str, original: &Value) {
    assert_success(
        &server
            .run(&[
                "member",
                "set-role",
                space,
                SECOND_USERNAME,
                "--role",
                "viewer",
            ])
            .await,
    );
    for arguments in [
        vec!["knowledge", "history", space, "KN-1"],
        vec!["knowledge", "show", space, "guide.md", "--version", "1"],
        vec!["knowledge", "deleted", space],
    ] {
        let mut arguments = arguments;
        arguments.extend(["--output", "json"]);
        let output = server.run_as(SECOND_USERNAME, &arguments).await;
        assert_success(&output);
        assert!(output.stderr.is_empty());
        match arguments[1] {
            "show" => assert_eq!(&body(&output)["data"], original),
            "history" => {
                assert_eq!(body(&output)["data"].as_array().unwrap().len(), 1);
                assert_eq!(body(&output)["data"][0]["version"], 1);
            }
            _ => assert_eq!(body(&output), json!({"data": [], "meta": {}})),
        }
    }
}

pub async fn assert_version_usage(server: &mut Server, space: &str) {
    server.stop();
    for version in ["old", "1.5", "9223372036854775808"] {
        for args in [
            vec![
                "knowledge",
                "rollback",
                space,
                "KN-1",
                version,
                "--output",
                "json",
            ],
            vec![
                "knowledge",
                "show",
                space,
                "KN-1",
                "--version",
                version,
                "--output",
                "json",
            ],
        ] {
            assert_usage(&server.run(&args).await);
        }
    }
}

pub async fn assert_key_history(server: &Server, space: &str) {
    for entry in [
        "KN-2",
        "kn-1",
        "KN-01",
        "KN-0",
        "KN-+1",
        "KN-1.md",
        "KN-1a",
        "KN-9223372036854775808",
    ] {
        let original = show_entry(server, space, entry).await;
        assert_eq!(read_entry_version(server, space, entry, 1).await, original);
        let history = knowledge_json(server, &["history", space, entry]).await;
        assert_eq!(history.as_array().unwrap().len(), 1);
        assert_eq!(history[0]["version"], 1);
    }
    let restored = knowledge_json(server, &["rollback", space, "KN-01", "1"]).await;
    assert_eq!(restored["version"], 2);
    assert_eq!(restored["content"], "Named content");
    assert_eq!(show_entry(server, space, "KN-01").await, restored);
}

pub fn assert_rollback_content(restored: &Value, original: &Value, name: &str, version: i64) {
    assert_eq!(restored["name"], name);
    assert_eq!(restored["key"], original["key"]);
    assert_eq!(restored["version"], version);
    assert_eq!(restored["kind"], "content");
    assert_eq!(restored["content"], original["content"]);
    assert_eq!(restored["summary"], original["summary"]);
    assert_eq!(restored["applied_by"], original["applied_by"]);
    assert!(restored["applied_at"].as_i64().unwrap() >= original["applied_at"].as_i64().unwrap());
}
