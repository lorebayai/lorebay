use std::fs;

use reqwest::Method;
use serde_json::{Value, json};

use crate::helpers::{
    SECOND_USERNAME, assert_empty_person_request_search, assert_person_request_error,
    assert_person_request_fields, assert_person_request_pages, assert_person_request_sources,
    assert_person_request_usage, assert_success, assert_usage, assert_withdrawn_request, body,
    contributor, create_entry, create_person_request, create_task, knowledge_context,
    knowledge_json, person_request_json, propose_rollback, read_entry_version, show_entry,
    show_person_request, submit_agent_person_request,
};

#[tokio::test]
async fn person_request_create_submits_pending_person_request() {
    let (server, space) = knowledge_context().await;
    let content = "# Guide\n\t**source**\r\u{1b}[31m\u{202e}";
    let summary = "Summary\n\t\u{1b}[31m";
    let person_request = create_person_request(&server, &space, "guide.md", summary, content).await;
    assert_eq!(person_request["key"], "RQ-1");
    assert_eq!(person_request["action"], "create");
    assert_eq!(person_request["status"], "pending");
    assert_eq!(person_request["new_name"], "guide.md");
    assert_eq!(person_request["new_summary"], summary);
    assert_eq!(person_request["new_content"], content);
    assert_eq!(person_request["requested_by"], server.username);
    for field in [
        "target",
        "requester_agent_name",
        "from_task_key",
        "base_version",
        "decided_by",
        "decided_at",
        "reject_reason",
    ] {
        assert_eq!(person_request[field], Value::Null, "{field}");
    }
    assert!(person_request["created_at"].is_i64());
    assert_eq!(person_request["outdated"], false);
    assert_eq!(
        show_person_request(&server, &space, "RQ-1").await,
        person_request
    );
    let shown = server.run(&["request", "show", &space, "RQ-1"]).await;
    assert_person_request_fields(&shown, &person_request, "guide.md");
    let text = String::from_utf8(shown.stdout).unwrap();
    assert!(text.contains("Proposed summary: Summary\\n\\t\\u{1b}[31m\n"));
    assert!(text.contains("Content: # Guide\n\t**source**\\r\\u{1b}[31m\\u{202e}\n"));
    assert!(!text.contains(['\u{1b}', '\r', '\u{202e}']));
    assert_eq!(knowledge_json(&server, &["list", &space]).await, json!([]));
    let created = server
        .run(&[
            "request",
            "create",
            &space,
            "human.md",
            "--summary",
            "Human",
            "--content",
            content,
        ])
        .await;
    assert_person_request_fields(
        &created,
        &show_person_request(&server, &space, "RQ-2").await,
        "human.md",
    );
}

#[tokio::test]
async fn person_request_delete_submits_person_request_without_confirmation() {
    let (server, space) = knowledge_context().await;
    let first = create_entry(&server, &space, "guides/a ?#%+.md", "First", "Keep").await;
    let second = create_entry(&server, &space, "KN-1", "Key-shaped name", "Second").await;
    let third = create_entry(&server, &space, "KN-01", "Name", "Third").await;
    for (index, (entry, expected, target)) in [
        ("guides/A ?#%+.md", &first, "KN-1 guides/a ?#%+.md"),
        ("KN-1", &first, "KN-1 guides/a ?#%+.md"),
        ("KN-2", &second, "KN-2 KN-1"),
        ("kn-1", &second, "KN-2 KN-1"),
        ("KN-01", &third, "KN-3 KN-01"),
    ]
    .into_iter()
    .enumerate()
    {
        let mode = if index % 2 == 0 { "human" } else { "json" };
        let output = server
            .run(&["request", "delete", &space, entry, "--output", mode])
            .await;
        assert_success(&output);
        assert!(output.stderr.is_empty());
        let key = format!("RQ-{}", index + 1);
        let person_request = show_person_request(&server, &space, &key).await;
        assert_eq!(person_request["status"], "pending");
        assert_eq!(person_request["action"], "delete");
        assert_eq!(
            person_request["target"],
            json!({"key": expected["key"], "name": expected["name"]})
        );
        assert_eq!(
            &show_entry(&server, &space, expected["key"].as_str().unwrap()).await,
            expected
        );
        if mode == "json" {
            assert_eq!(body(&output), json!({"data": person_request, "meta": {}}));
        } else {
            assert_person_request_fields(&output, &person_request, target);
        }
    }
    person_request_json(&server, &["approve", &space, "RQ-1"]).await;
    assert_eq!(
        show_entry(&server, &space, "KN-1").await["kind"],
        "deletion"
    );
}

#[tokio::test]
async fn person_request_change_proposes_old_version_as_rollback() {
    let (server, space) = knowledge_context().await;
    create_entry(
        &server,
        &space,
        "guide.md",
        "Old summary",
        "# Old\n\tcontent",
    )
    .await;
    let current = knowledge_json(
        &server,
        &[
            "change",
            &space,
            "KN-1",
            "--name",
            "renamed.md",
            "--summary",
            "New",
            "--content",
            "New content",
        ],
    )
    .await;
    contributor(&server, &space).await;
    let (old, person_request) = propose_rollback(&server, &space).await;
    assert_eq!(person_request["status"], "pending");
    assert_eq!(person_request["action"], "change");
    assert_eq!(person_request["new_name"], Value::Null);
    assert_eq!(person_request["new_summary"], old["summary"]);
    assert_eq!(person_request["new_content"], old["content"]);
    assert_eq!(person_request["base_version"], 2);
    assert_eq!(show_entry(&server, &space, "KN-1").await, current);
    person_request_json(&server, &["approve", &space, "RQ-1"]).await;
    let restored = show_entry(&server, &space, "KN-1").await;
    assert_eq!(restored["version"], 3);
    assert_eq!(restored["name"], "renamed.md");
    assert_eq!(restored["summary"], old["summary"]);
    assert_eq!(restored["content"], old["content"]);
    assert_eq!(
        read_entry_version(&server, &space, "KN-1", 2).await,
        current
    );
}

#[tokio::test]
async fn person_request_list_filters_by_status() {
    let (mut server, space) = knowledge_context().await;
    let name = r#"guide "quoted"\path.md"#;
    create_entry(&server, &space, name, "Guide", "Keep").await;
    for name in ["one.md", "two.md", "three.md", "four.md"] {
        create_person_request(&server, &space, name, "Summary", "Content").await;
    }
    for key in ["RQ-1", "RQ-2"] {
        person_request_json(&server, &["approve", &space, key]).await;
    }
    for key in ["RQ-3", "RQ-4"] {
        person_request_json(&server, &["reject", &space, key, "--reason", "Superseded"]).await;
    }
    person_request_json(
        &server,
        &["change", &space, "KN-1", "--summary", "New\n\t\u{1b}[31m"],
    )
    .await;
    person_request_json(&server, &["delete", &space, "KN-1"]).await;
    create_person_request(
        &server,
        &space,
        r#"draft "quoted"\new.md"#,
        "Draft",
        "Content",
    )
    .await;
    for (status, expected) in [
        (
            "pending",
            vec![
                ("RQ-7", r#"draft "quoted"\new.md"#),
                ("RQ-6", r#"KN-1 guide "quoted"\path.md"#),
                ("RQ-5", r#"KN-1 guide "quoted"\path.md"#),
            ],
        ),
        (
            "approved",
            vec![("RQ-2", "KN-3 two.md"), ("RQ-1", "KN-2 one.md")],
        ),
        ("rejected", vec![("RQ-4", "four.md"), ("RQ-3", "three.md")]),
    ] {
        assert_person_request_pages(
            &server,
            &["request", "list", &space, "--status", status],
            &expected,
        )
        .await;
    }
    let list = server
        .run(&["request", "list", &space, "--status", "pending"])
        .await;
    assert_success(&list);
    let text = String::from_utf8(list.stdout).unwrap();
    assert!(text.starts_with("Key\tAction\tStatus\tTarget\tProposed summary\n"));
    assert!(text.contains(r#"KN-1 guide "quoted"\path.md"#));
    assert!(text.contains("RQ-7\tcreate\tpending\tdraft \"quoted\"\\new.md\tDraft"));
    assert!(text.contains("New\\n\\t\\u{1b}[31m"));
    assert_person_request_usage(&mut server, &space).await;
}

#[tokio::test]
async fn person_request_list_filters_by_keyword() {
    let (server, space) = knowledge_context().await;
    let keyword = "needle 'quoted' %_\\ &?";
    for (name, summary) in [
        ("one.md", keyword),
        ("two.md", keyword),
        (keyword, "Summary"),
        ("other.md", "Other"),
    ] {
        create_person_request(&server, &space, name, summary, "Content").await;
    }
    person_request_json(&server, &["approve", &space, "RQ-1"]).await;
    person_request_json(
        &server,
        &["reject", &space, "RQ-2", "--reason", "Superseded"],
    )
    .await;
    create_entry(&server, &space, keyword, "Target", "Keep").await;
    person_request_json(&server, &["change", &space, "KN-2", "--content", "Changed"]).await;
    person_request_json(&server, &["delete", &space, "KN-2"]).await;
    assert_person_request_pages(
        &server,
        &["request", "list", &space, "--keyword", keyword],
        &[
            ("RQ-6", r"KN-2 needle 'quoted' %_\ &?"),
            ("RQ-5", r"KN-2 needle 'quoted' %_\ &?"),
            ("RQ-3", r"needle 'quoted' %_\ &?"),
            ("RQ-2", "two.md"),
            ("RQ-1", "KN-1 one.md"),
        ],
    )
    .await;
    assert_person_request_pages(
        &server,
        &[
            "request",
            "list",
            &space,
            "--status",
            "pending",
            "--keyword",
            keyword,
        ],
        &[
            ("RQ-6", r"KN-2 needle 'quoted' %_\ &?"),
            ("RQ-5", r"KN-2 needle 'quoted' %_\ &?"),
            ("RQ-3", r"needle 'quoted' %_\ &?"),
        ],
    )
    .await;
    assert_empty_person_request_search(&server, &space).await;
}

#[tokio::test]
async fn person_request_create_reads_content_from_file() {
    let (server, space) = knowledge_context().await;
    let path = server.directory.path().join("request.md");
    let content = format!("\u{feff}{}", "# Guide\r\n\t\u{00e9} text\n".repeat(16_000));
    assert!(content.len() > 200 * 1024);
    fs::write(&path, &content).unwrap();
    let output = server
        .run(&[
            "request",
            "create",
            &space,
            "guide.md",
            "--summary",
            "From file",
            "--content-file",
            path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .await;
    assert_success(&output);
    let person_request = show_person_request(&server, &space, "RQ-1").await;
    assert_eq!(body(&output), json!({"data": person_request, "meta": {}}));
    assert_eq!(person_request["new_content"], content);
    assert_eq!(person_request["status"], "pending");
    assert_eq!(knowledge_json(&server, &["list", &space]).await, json!([]));
    assert_person_request_sources(&server, &space, &path).await;
}

#[tokio::test]
async fn person_request_show_marks_outdated() {
    let (server, space) = knowledge_context().await;
    create_entry(
        &server,
        &space,
        r#"guides/a "quoted"\path ?#%+.md"#,
        "Original",
        "Old",
    )
    .await;
    let person_request = person_request_json(
        &server,
        &[
            "change",
            &space,
            r#"guides/A "quoted"\path ?#%+.md"#,
            "--name",
            "renamed.md",
            "--content",
            "# Proposed\n\tcontent",
        ],
    )
    .await;
    assert_eq!(person_request["outdated"], false);
    assert_eq!(person_request["new_summary"], Value::Null);
    assert_eq!(person_request["target"]["key"], "KN-1");
    knowledge_json(
        &server,
        &["change", &space, "KN-1", "--summary", "Intervening"],
    )
    .await;
    let shown = show_person_request(&server, &space, "RQ-1").await;
    assert_eq!(shown["outdated"], true);
    let human = server.run(&["request", "show", &space, "RQ-1"]).await;
    assert_person_request_fields(&human, &shown, r#"KN-1 guides/a "quoted"\path ?#%+.md"#);
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("Outdated: true\n")
    );
    person_request_json(&server, &["revise", &space, "RQ-1"]).await;
    let approved = server
        .run(&["request", "approve", &space, "RQ-1", "--revision", "2"])
        .await;
    let decided = show_person_request(&server, &space, "RQ-1").await;
    assert_person_request_fields(&approved, &decided, "KN-1 renamed.md");
    assert_eq!(decided["status"], "approved");
    assert_eq!(decided["decided_by"], server.username);
    assert!(decided["decided_at"].is_i64());
    assert!(decided.get("outdated").is_none());
    let entry = show_entry(&server, &space, "KN-1").await;
    assert_eq!(entry["name"], "renamed.md");
    assert_eq!(entry["summary"], "Intervening");
    assert_eq!(entry["content"], person_request["new_content"]);
    assert_eq!(entry["version"], 3);
}

#[tokio::test]
async fn person_request_approve_makes_agent_person_request_readable_as_knowledge() {
    let (server, space) = knowledge_context().await;
    contributor(&server, &space).await;
    let agent_name = "Writer\n\t\u{1b}[31m";
    let agent = server.agent_as(agent_name, SECOND_USERNAME).await;
    let task = create_task(&server, &space, &agent, "Write guide").await;
    let mut person_request = submit_agent_person_request(&server, &space, &agent, &task).await;
    assert_eq!(person_request["task_title"], "Write guide");
    person_request.as_object_mut().unwrap().remove("task_title");
    assert_eq!(person_request["requested_by"], SECOND_USERNAME);
    assert_eq!(person_request["requester_agent_name"], agent_name);
    assert_eq!(person_request["from_task_key"], task["key"]);
    assert_eq!(
        show_person_request(&server, &space, "RQ-1").await,
        person_request
    );
    let human = server.run(&["request", "show", &space, "RQ-1"]).await;
    assert_person_request_fields(&human, &person_request, "agent.md");
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("Submitting agent: Writer\\n\\t\\u{1b}[31m\n")
    );
    let approved = person_request_json(&server, &["approve", &space, "RQ-1"]).await;
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["decided_by"], server.username);
    assert!(
        approved["decided_at"].as_i64().unwrap() >= person_request["created_at"].as_i64().unwrap()
    );
    assert_eq!(show_person_request(&server, &space, "RQ-1").await, approved);
    let read = server
        .json(
            Method::GET,
            &format!("/api/v1/spaces/{space}/knowledge/by-key/KN-1"),
            Value::Null,
            agent["access_token"].as_str(),
        )
        .await;
    assert_eq!(read["data"], show_entry(&server, &space, "KN-1").await);
    assert_eq!(read["data"]["name"], person_request["new_name"]);
    assert_eq!(read["data"]["summary"], person_request["new_summary"]);
    assert_eq!(read["data"]["content"], person_request["new_content"]);
    assert_eq!(read["data"]["version"], 1);
}

#[tokio::test]
async fn person_request_reject_requires_reason() {
    let (server, space) = knowledge_context().await;
    let person_request =
        create_person_request(&server, &space, "guide.md", "Summary", "Content").await;
    assert_usage(
        &server
            .run(&[
                "request",
                "reject",
                &space,
                "RQ-1",
                "--output",
                "json",
                "--revision",
                "1",
            ])
            .await,
    );
    for mode in ["human", "json"] {
        for reason in ["", " "] {
            let output = server
                .run(&[
                    "request",
                    "reject",
                    &space,
                    "RQ-1",
                    "--reason",
                    reason,
                    "--output",
                    mode,
                    "--revision",
                    "1",
                ])
                .await;
            assert_person_request_error(
                &server,
                &output,
                (
                    &space,
                    "RQ-1",
                    "reject",
                    json!({"revision":1,"reason": reason}),
                ),
                mode,
            )
            .await;
            assert_eq!(
                show_person_request(&server, &space, "RQ-1").await,
                person_request
            );
        }
    }
    assert_eq!(knowledge_json(&server, &["list", &space]).await, json!([]));
}

#[tokio::test]
async fn person_request_reject_withdraws_own_person_request() {
    let (server, space) = knowledge_context().await;
    contributor(&server, &space).await;
    let reason = "Superseded\n\t\u{1b}[31m";
    for (index, mode) in ["human", "json"].into_iter().enumerate() {
        let created = server
            .run_as(
                SECOND_USERNAME,
                &[
                    "request",
                    "create",
                    &space,
                    "guide.md",
                    "--summary",
                    "Summary",
                    "--content",
                    "Content",
                    "--output",
                    "json",
                ],
            )
            .await;
        assert_success(&created);
        let key = format!("RQ-{}", index + 1);
        let output = server
            .run_as(
                SECOND_USERNAME,
                &[
                    "request",
                    "reject",
                    &space,
                    &key,
                    "--reason",
                    reason,
                    "--output",
                    mode,
                    "--revision",
                    "1",
                ],
            )
            .await;
        assert_success(&output);
        assert!(output.stderr.is_empty());
        let person_request = show_person_request(&server, &space, &key).await;
        assert_eq!(person_request["status"], "rejected");
        assert_eq!(person_request["requested_by"], SECOND_USERNAME);
        assert_eq!(person_request["decided_by"], SECOND_USERNAME);
        assert!(person_request["decided_at"].is_i64());
        assert_eq!(person_request["reject_reason"], reason);
        if mode == "json" {
            assert_eq!(body(&output), json!({"data": person_request, "meta": {}}));
        } else {
            assert_person_request_fields(&output, &person_request, "guide.md");
        }
        assert_withdrawn_request(&server, &space, &key, &person_request).await;
    }
    assert_eq!(knowledge_json(&server, &["list", &space]).await, json!([]));
}

#[tokio::test]
async fn person_request_approve_fails_with_server_error_when_contributor_approves_own() {
    let (server, space) = knowledge_context().await;
    contributor(&server, &space).await;
    let created = server
        .run_as(
            SECOND_USERNAME,
            &[
                "request",
                "create",
                &space,
                "guide.md",
                "--summary",
                "Summary",
                "--content",
                "Content",
                "--output",
                "json",
            ],
        )
        .await;
    assert_success(&created);
    let agent = server.agent_as("Writer", SECOND_USERNAME).await;
    let task = create_task(&server, &space, &agent, "Write guide").await;
    submit_agent_person_request(&server, &space, &agent, &task).await;
    for key in ["RQ-1", "RQ-2"] {
        for mode in ["human", "json"] {
            let output = server
                .run_as(
                    SECOND_USERNAME,
                    &[
                        "request",
                        "approve",
                        &space,
                        key,
                        "--output",
                        mode,
                        "--revision",
                        "1",
                    ],
                )
                .await;
            assert_person_request_error(
                &server,
                &output,
                (&space, key, "approve", json!({"revision":1})),
                mode,
            )
            .await;
            assert_eq!(
                show_person_request(&server, &space, key).await["status"],
                "pending"
            );
        }
    }
    assert_eq!(knowledge_json(&server, &["list", &space]).await, json!([]));
}
