use std::fs;

use reqwest::Method;
use serde_json::{Value, json};

use crate::helpers::{
    SECOND_USERNAME, assert_empty_proposal_search, assert_proposal_error, assert_proposal_fields,
    assert_proposal_pages, assert_proposal_sources, assert_proposal_usage, assert_success,
    assert_usage, body, contributor, create_entry, create_proposal, create_task, knowledge_context,
    knowledge_json, proposal_json, propose_rollback, read_entry_version, show_entry, show_proposal,
    submit_agent_proposal,
};

#[tokio::test]
async fn proposal_create_submits_pending_proposal() {
    let (server, space) = knowledge_context().await;
    let content = "# Guide\n\t**source**\r\u{1b}[31m\u{202e}";
    let summary = "Summary\n\t\u{1b}[31m";
    let proposal = create_proposal(&server, &space, "guide.md", summary, content).await;
    assert_eq!(proposal["key"], "PP-1");
    assert_eq!(proposal["kind"], "create");
    assert_eq!(proposal["status"], "pending");
    assert_eq!(proposal["new_name"], "guide.md");
    assert_eq!(proposal["new_summary"], summary);
    assert_eq!(proposal["new_content"], content);
    assert_eq!(proposal["proposed_by"], server.username);
    for field in [
        "target",
        "proposer_agent_name",
        "from_task_key",
        "base_version",
        "decided_by",
        "decided_at",
        "reject_reason",
    ] {
        assert_eq!(proposal[field], Value::Null, "{field}");
    }
    assert!(proposal["created_at"].is_i64());
    assert_eq!(proposal["changed_since_proposed"], false);
    assert_eq!(show_proposal(&server, &space, "PP-1").await, proposal);
    let shown = server.run(&["proposal", "show", &space, "PP-1"]).await;
    assert_proposal_fields(&shown, &proposal);
    let text = String::from_utf8(shown.stdout).unwrap();
    assert!(text.contains("Proposed summary: Summary\\n\\t\\u{1b}[31m\n"));
    assert!(text.contains("Content: # Guide\n\t**source**\\r\\u{1b}[31m\\u{202e}\n"));
    assert!(!text.contains(['\u{1b}', '\r', '\u{202e}']));
    assert_eq!(knowledge_json(&server, &["list", &space]).await, json!([]));
    let created = server
        .run(&[
            "proposal",
            "create",
            &space,
            "human.md",
            "--summary",
            "Human",
            "--content",
            content,
        ])
        .await;
    assert_proposal_fields(&created, &show_proposal(&server, &space, "PP-2").await);
}

#[tokio::test]
async fn proposal_delete_submits_proposal_without_confirmation() {
    let (server, space) = knowledge_context().await;
    let first = create_entry(&server, &space, "guides/a ?#%+.md", "First", "Keep").await;
    let second = create_entry(&server, &space, "KN-1", "Key-shaped name", "Second").await;
    let third = create_entry(&server, &space, "KN-01", "Name", "Third").await;
    for (index, (entry, expected)) in [
        ("guides/A ?#%+.md", &first),
        ("KN-1", &first),
        ("KN-2", &second),
        ("kn-1", &second),
        ("KN-01", &third),
    ]
    .into_iter()
    .enumerate()
    {
        let mode = if index % 2 == 0 { "human" } else { "json" };
        let output = server
            .run(&["proposal", "delete", &space, entry, "--output", mode])
            .await;
        assert_success(&output);
        assert!(output.stderr.is_empty());
        let key = format!("PP-{}", index + 1);
        let proposal = show_proposal(&server, &space, &key).await;
        assert_eq!(proposal["status"], "pending");
        assert_eq!(proposal["kind"], "delete");
        assert_eq!(
            proposal["target"],
            json!({"key": expected["key"], "name": expected["name"]})
        );
        assert_eq!(
            &show_entry(&server, &space, expected["key"].as_str().unwrap()).await,
            expected
        );
        if mode == "json" {
            assert_eq!(body(&output), json!({"data": proposal, "meta": {}}));
        } else {
            assert_proposal_fields(&output, &proposal);
        }
    }
    proposal_json(&server, &["approve", &space, "PP-1"]).await;
    assert_eq!(
        show_entry(&server, &space, "KN-1").await["kind"],
        "deletion"
    );
}

#[tokio::test]
async fn proposal_change_proposes_old_version_as_rollback() {
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
    let (old, proposal) = propose_rollback(&server, &space).await;
    assert_eq!(proposal["status"], "pending");
    assert_eq!(proposal["kind"], "change");
    assert_eq!(proposal["new_name"], Value::Null);
    assert_eq!(proposal["new_summary"], old["summary"]);
    assert_eq!(proposal["new_content"], old["content"]);
    assert_eq!(proposal["base_version"], 2);
    assert_eq!(show_entry(&server, &space, "KN-1").await, current);
    proposal_json(&server, &["approve", &space, "PP-1"]).await;
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
async fn proposal_list_filters_by_status() {
    let (mut server, space) = knowledge_context().await;
    create_entry(&server, &space, "guide.md", "Guide", "Keep").await;
    for name in ["one.md", "two.md", "three.md", "four.md"] {
        create_proposal(&server, &space, name, "Summary", "Content").await;
    }
    for key in ["PP-1", "PP-2"] {
        proposal_json(&server, &["approve", &space, key]).await;
    }
    for key in ["PP-3", "PP-4"] {
        proposal_json(&server, &["reject", &space, key, "--reason", "Superseded"]).await;
    }
    proposal_json(
        &server,
        &["change", &space, "KN-1", "--summary", "New\n\t\u{1b}[31m"],
    )
    .await;
    proposal_json(&server, &["delete", &space, "KN-1"]).await;
    for (status, expected) in [
        ("pending", vec!["PP-6", "PP-5"]),
        ("approved", vec!["PP-2", "PP-1"]),
        ("rejected", vec!["PP-4", "PP-3"]),
    ] {
        assert_proposal_pages(
            &server,
            &["proposal", "list", &space, "--status", status],
            &expected,
        )
        .await;
    }
    let list = server
        .run(&["proposal", "list", &space, "--status", "pending"])
        .await;
    assert_success(&list);
    let text = String::from_utf8(list.stdout).unwrap();
    assert!(text.starts_with("Key\tKind\tStatus\tTarget\tProposed summary\n"));
    assert!(text.contains("KN-1"));
    assert!(text.contains("guide.md"));
    assert!(text.contains("New\\n\\t\\u{1b}[31m"));
    assert_proposal_usage(&mut server, &space).await;
}

#[tokio::test]
async fn proposal_list_filters_by_keyword() {
    let (server, space) = knowledge_context().await;
    let keyword = "needle 'quoted' %_\\ &?";
    for (name, summary) in [
        ("one.md", keyword),
        ("two.md", keyword),
        (keyword, "Summary"),
        ("other.md", "Other"),
    ] {
        create_proposal(&server, &space, name, summary, "Content").await;
    }
    proposal_json(&server, &["approve", &space, "PP-1"]).await;
    proposal_json(
        &server,
        &["reject", &space, "PP-2", "--reason", "Superseded"],
    )
    .await;
    create_entry(&server, &space, keyword, "Target", "Keep").await;
    proposal_json(&server, &["change", &space, "KN-2", "--content", "Changed"]).await;
    proposal_json(&server, &["delete", &space, "KN-2"]).await;
    assert_proposal_pages(
        &server,
        &["proposal", "list", &space, "--keyword", keyword],
        &["PP-6", "PP-5", "PP-3", "PP-2", "PP-1"],
    )
    .await;
    assert_proposal_pages(
        &server,
        &[
            "proposal",
            "list",
            &space,
            "--status",
            "pending",
            "--keyword",
            keyword,
        ],
        &["PP-6", "PP-5", "PP-3"],
    )
    .await;
    assert_empty_proposal_search(&server, &space).await;
}

#[tokio::test]
async fn proposal_create_reads_content_from_file() {
    let (server, space) = knowledge_context().await;
    let path = server.directory.path().join("proposal.md");
    let content = format!("\u{feff}{}", "# Guide\r\n\t\u{00e9} text\n".repeat(16_000));
    assert!(content.len() > 200 * 1024);
    fs::write(&path, &content).unwrap();
    let output = server
        .run(&[
            "proposal",
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
    let proposal = show_proposal(&server, &space, "PP-1").await;
    assert_eq!(body(&output), json!({"data": proposal, "meta": {}}));
    assert_eq!(proposal["new_content"], content);
    assert_eq!(proposal["status"], "pending");
    assert_eq!(knowledge_json(&server, &["list", &space]).await, json!([]));
    assert_proposal_sources(&server, &space, &path).await;
}

#[tokio::test]
async fn proposal_show_marks_changed_since_proposed() {
    let (server, space) = knowledge_context().await;
    create_entry(&server, &space, "guides/a ?#%+.md", "Original", "Old").await;
    let proposal = proposal_json(
        &server,
        &[
            "change",
            &space,
            "guides/A ?#%+.md",
            "--name",
            "renamed.md",
            "--content",
            "# Proposed\n\tcontent",
        ],
    )
    .await;
    assert_eq!(proposal["changed_since_proposed"], false);
    assert_eq!(proposal["new_summary"], Value::Null);
    assert_eq!(proposal["target"]["key"], "KN-1");
    knowledge_json(
        &server,
        &["change", &space, "KN-1", "--summary", "Intervening"],
    )
    .await;
    let shown = show_proposal(&server, &space, "PP-1").await;
    assert_eq!(shown["changed_since_proposed"], true);
    let human = server.run(&["proposal", "show", &space, "PP-1"]).await;
    assert_proposal_fields(&human, &shown);
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("Changed since proposed: true\n")
    );
    let approved = server.run(&["proposal", "approve", &space, "PP-1"]).await;
    let decided = show_proposal(&server, &space, "PP-1").await;
    assert_proposal_fields(&approved, &decided);
    assert_eq!(decided["status"], "approved");
    assert_eq!(decided["decided_by"], server.username);
    assert!(decided["decided_at"].is_i64());
    assert!(decided.get("changed_since_proposed").is_none());
    let entry = show_entry(&server, &space, "KN-1").await;
    assert_eq!(entry["name"], "renamed.md");
    assert_eq!(entry["summary"], "Intervening");
    assert_eq!(entry["content"], proposal["new_content"]);
    assert_eq!(entry["version"], 3);
}

#[tokio::test]
async fn proposal_approve_makes_agent_proposal_readable_as_knowledge() {
    let (server, space) = knowledge_context().await;
    contributor(&server, &space).await;
    let agent_name = "Writer\n\t\u{1b}[31m";
    let agent = server.agent_as(agent_name, SECOND_USERNAME).await;
    let task = create_task(&server, &space, &agent, "Write guide").await;
    let mut proposal = submit_agent_proposal(&server, &space, &agent, &task).await;
    assert_eq!(proposal["task_title"], "Write guide");
    proposal.as_object_mut().unwrap().remove("task_title");
    assert_eq!(proposal["proposed_by"], SECOND_USERNAME);
    assert_eq!(proposal["proposer_agent_name"], agent_name);
    assert_eq!(proposal["from_task_key"], task["key"]);
    assert_eq!(show_proposal(&server, &space, "PP-1").await, proposal);
    let human = server.run(&["proposal", "show", &space, "PP-1"]).await;
    assert_proposal_fields(&human, &proposal);
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("Agent name: Writer\\n\\t\\u{1b}[31m\n")
    );
    let approved = proposal_json(&server, &["approve", &space, "PP-1"]).await;
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["decided_by"], server.username);
    assert!(approved["decided_at"].as_i64().unwrap() >= proposal["created_at"].as_i64().unwrap());
    assert_eq!(show_proposal(&server, &space, "PP-1").await, approved);
    let read = server
        .json(
            Method::GET,
            &format!("/api/v1/spaces/{space}/knowledge/by-key/KN-1"),
            Value::Null,
            agent["access_token"].as_str(),
        )
        .await;
    assert_eq!(read["data"], show_entry(&server, &space, "KN-1").await);
    assert_eq!(read["data"]["name"], proposal["new_name"]);
    assert_eq!(read["data"]["summary"], proposal["new_summary"]);
    assert_eq!(read["data"]["content"], proposal["new_content"]);
    assert_eq!(read["data"]["version"], 1);
}

#[tokio::test]
async fn proposal_reject_requires_reason() {
    let (server, space) = knowledge_context().await;
    let proposal = create_proposal(&server, &space, "guide.md", "Summary", "Content").await;
    assert_usage(
        &server
            .run(&["proposal", "reject", &space, "PP-1", "--output", "json"])
            .await,
    );
    for mode in ["human", "json"] {
        for reason in ["", " "] {
            let output = server
                .run(&[
                    "proposal", "reject", &space, "PP-1", "--reason", reason, "--output", mode,
                ])
                .await;
            assert_proposal_error(
                &server,
                &output,
                (&space, "PP-1", "reject", json!({"reason": reason})),
                mode,
            )
            .await;
            assert_eq!(show_proposal(&server, &space, "PP-1").await, proposal);
        }
    }
    assert_eq!(knowledge_json(&server, &["list", &space]).await, json!([]));
}

#[tokio::test]
async fn proposal_reject_withdraws_own_proposal() {
    let (server, space) = knowledge_context().await;
    contributor(&server, &space).await;
    let reason = "Superseded\n\t\u{1b}[31m";
    for (index, mode) in ["human", "json"].into_iter().enumerate() {
        let created = server
            .run_as(
                SECOND_USERNAME,
                &[
                    "proposal",
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
        let key = format!("PP-{}", index + 1);
        let output = server
            .run_as(
                SECOND_USERNAME,
                &[
                    "proposal", "reject", &space, &key, "--reason", reason, "--output", mode,
                ],
            )
            .await;
        assert_success(&output);
        assert!(output.stderr.is_empty());
        let proposal = show_proposal(&server, &space, &key).await;
        assert_eq!(proposal["status"], "rejected");
        assert_eq!(proposal["proposed_by"], SECOND_USERNAME);
        assert_eq!(proposal["decided_by"], SECOND_USERNAME);
        assert!(proposal["decided_at"].is_i64());
        assert_eq!(proposal["reject_reason"], reason);
        if mode == "json" {
            assert_eq!(body(&output), json!({"data": proposal, "meta": {}}));
        } else {
            assert_proposal_fields(&output, &proposal);
        }
        let shown = server.run(&["proposal", "show", &space, &key]).await;
        assert_proposal_fields(&shown, &proposal);
        assert!(
            String::from_utf8(shown.stdout)
                .unwrap()
                .contains("Rejection reason: Superseded\\n\\t\\u{1b}[31m\n")
        );
    }
    assert_eq!(knowledge_json(&server, &["list", &space]).await, json!([]));
}

#[tokio::test]
async fn proposal_approve_fails_with_server_error_when_contributor_approves_own() {
    let (server, space) = knowledge_context().await;
    contributor(&server, &space).await;
    let created = server
        .run_as(
            SECOND_USERNAME,
            &[
                "proposal",
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
    submit_agent_proposal(&server, &space, &agent, &task).await;
    for key in ["PP-1", "PP-2"] {
        for mode in ["human", "json"] {
            let output = server
                .run_as(
                    SECOND_USERNAME,
                    &["proposal", "approve", &space, key, "--output", mode],
                )
                .await;
            assert_proposal_error(
                &server,
                &output,
                (&space, key, "approve", Value::Null),
                mode,
            )
            .await;
            assert_eq!(
                show_proposal(&server, &space, key).await["status"],
                "pending"
            );
        }
    }
    assert_eq!(knowledge_json(&server, &["list", &space]).await, json!([]));
}
