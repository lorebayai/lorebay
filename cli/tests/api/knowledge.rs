use std::fs;

use serde_json::json;

use crate::helpers::{
    SECOND_USERNAME, assert_cancelled, assert_content_file_limits, assert_empty_knowledge_search,
    assert_history_errors, assert_history_pages, assert_key_history, assert_knowledge_error,
    assert_knowledge_prefix_pages, assert_other_content_sources, assert_rollback_content,
    assert_rollback_refusals, assert_success, assert_usage, assert_version_usage,
    assert_viewer_reads_history, assert_viewer_reads_knowledge, body, confirm_deletion,
    content_input, create_entry, knowledge_context, knowledge_json, pages, piped_yes,
    read_entry_version, rollback_second_version, show_entry,
};

#[tokio::test]
async fn knowledge_create_then_show_prints_content() {
    let (server, space) = knowledge_context().await;
    let content = "# Guide\n\t**source**\r\n\u{1b}[31m\u{7}\u{202e}";
    let summary = "Summary\n\t\u{1b}[31m\u{202e}";
    let entry = create_entry(&server, &space, "guide.md", summary, content).await;
    assert_eq!(entry["key"], "KN-1");
    assert_eq!(entry["version"], 1);
    assert_eq!(entry["kind"], "content");
    assert_eq!(entry["summary"], summary);
    assert_eq!(entry["content"], content);
    assert_eq!(entry["applied_by"], server.username);
    assert!(entry["applied_at"].is_i64());
    assert_eq!(show_entry(&server, &space, "GUIDE.MD").await, entry);
    let shown = server.run(&["knowledge", "show", &space, "KN-1"]).await;
    assert_success(&shown);
    assert!(shown.stderr.is_empty());
    let text = String::from_utf8(shown.stdout).unwrap();
    assert!(text.contains("Summary: Summary\\n\\t\\u{1b}[31m\\u{202e}\n"));
    assert!(text.contains("Content: # Guide\n\t**source**\\r\n\\u{1b}[31m\\u{7}\\u{202e}\n"));
    assert!(!text.contains(['\u{1b}', '\r', '\u{7}', '\u{202e}', '\0']));
    let created = server
        .run(&[
            "knowledge",
            "create",
            &space,
            "second.md",
            "--summary",
            "Second",
            "--content",
            "# Source\n\ttext",
        ])
        .await;
    assert_success(&created);
    assert!(
        String::from_utf8(created.stdout)
            .unwrap()
            .contains("Content: # Source\n\ttext\n")
    );
}

#[tokio::test]
async fn knowledge_change_adds_version_shown_in_history() {
    let (server, space) = knowledge_context().await;
    let original = create_entry(&server, &space, "guide.md", "Original", "Old content").await;
    let mut versions = vec![original];
    for (option, value, version, summary, content) in [
        (
            "--content",
            "# New\n\tcontent",
            2,
            "Original",
            "# New\n\tcontent",
        ),
        (
            "--summary",
            "New summary",
            3,
            "New summary",
            "# New\n\tcontent",
        ),
    ] {
        let changed = knowledge_json(&server, &["change", &space, "guide.md", option, value]).await;
        let latest = show_entry(&server, &space, "KN-1").await;
        assert_eq!(changed, latest);
        assert_eq!(latest["version"], version);
        assert_eq!(latest["summary"], summary);
        assert_eq!(latest["content"], content);
        versions.push(latest);
    }
    let changed = server
        .run(&[
            "knowledge",
            "change",
            &space,
            "KN-1",
            "--summary",
            "Human",
            "--content",
            "# Human\n\tcontent",
            "--version",
            "3",
        ])
        .await;
    assert_success(&changed);
    assert!(
        String::from_utf8(changed.stdout)
            .unwrap()
            .contains("Content: # Human\n\tcontent\n")
    );
    let latest = show_entry(&server, &space, "KN-1").await;
    assert_eq!(latest["version"], 4);
    versions.push(latest);
    for entry in ["guide.md", "KN-1"] {
        assert_history_pages(&server, &space, entry, &versions).await;
    }
}

#[tokio::test]
async fn knowledge_change_renames_entry() {
    let (server, space) = knowledge_context().await;
    let entry = create_entry(
        &server,
        &space,
        "guides/old ?#&%+name.md",
        "Guide",
        "Keep content",
    )
    .await;
    let name = "guides/new ?#&%+name-\u{00e9}.md";
    let renamed = server
        .run(&[
            "knowledge",
            "change",
            &space,
            "guides/OLD ?#&%+name.md",
            "--name",
            name,
            "--output",
            "json",
            "--version",
            "1",
        ])
        .await;
    assert_success(&renamed);
    let mut expected = entry;
    expected["name"] = json!(name);
    assert_eq!(body(&renamed), json!({"data": expected, "meta": {}}));
    assert_eq!(show_entry(&server, &space, name).await, expected);
    assert_eq!(show_entry(&server, &space, "KN-1").await, expected);
    let old = server
        .run(&[
            "knowledge",
            "show",
            &space,
            "guides/old ?#&%+name.md",
            "--output",
            "json",
        ])
        .await;
    assert_knowledge_error(&old, "json", "knowledge.not_found");
}

#[tokio::test]
async fn knowledge_list_prints_entries_and_prefixes() {
    let (server, space) = knowledge_context().await;
    for name in ["a.md", "Deploy/a.md", "deploy/nested/b.md", "z.md"] {
        create_entry(&server, &space, name, "Summary\n\t\u{1b}[31m", "Content").await;
    }
    let items = pages(
        &server,
        &["knowledge", "list", &space],
        "name",
        &["a.md", "Deploy/", "z.md"],
    )
    .await;
    assert_eq!(
        items,
        json!([
            {"name": "a.md", "key": "KN-1", "summary": "Summary\n\t\u{1b}[31m"},
            {"name": "Deploy/"},
            {"name": "z.md", "key": "KN-4", "summary": "Summary\n\t\u{1b}[31m"}
        ])
        .as_array()
        .unwrap()
        .clone()
    );
    assert_knowledge_prefix_pages(&server, &space).await;
}

#[tokio::test]
async fn knowledge_list_searches_by_keyword() {
    let (server, space) = knowledge_context().await;
    let keyword = "needle 'quoted' %_\\ &?";
    for (name, summary, content) in [
        ("a.md", keyword, "Text"),
        ("nested/b.md", "Summary", keyword),
        ("c.md", "Summary", "Other"),
    ] {
        create_entry(&server, &space, name, summary, content).await;
    }
    let items = pages(
        &server,
        &["knowledge", "list", &space, "--keyword", keyword],
        "name",
        &["a.md", "nested/b.md"],
    )
    .await;
    assert_eq!(items.len(), 2);
    for mode in ["human", "json"] {
        let first = server
            .run(&[
                "knowledge",
                "list",
                &space,
                "--keyword",
                keyword,
                "--page-size",
                "1",
                "--output",
                mode,
            ])
            .await;
        assert_success(&first);
        let hint = String::from_utf8(first.stderr).unwrap();
        assert!(
            hint.contains(" --keyword 'needle '\\''quoted'\\'' %_\\ &?' --page-size 1 --cursor '")
        );
        let refused = server
            .run(&[
                "knowledge",
                "list",
                &space,
                "--prefix",
                "nested/",
                "--keyword",
                keyword,
                "--output",
                mode,
            ])
            .await;
        assert_knowledge_error(&refused, mode, "request.invalid_query");
    }
    assert_empty_knowledge_search(&server, &space).await;
}

#[tokio::test]
async fn knowledge_delete_cancels_without_yes() {
    let (server, space) = knowledge_context().await;
    for mode in ["human", "json"] {
        let entry = create_entry(&server, &space, mode, "Keep until confirmed", "Content").await;
        for output in [
            server
                .run(&[
                    "knowledge",
                    "delete",
                    &space,
                    mode,
                    "--output",
                    mode,
                    "--version",
                    "1",
                ])
                .await,
            piped_yes(
                &server,
                &[
                    "knowledge",
                    "delete",
                    &space,
                    mode,
                    "--output",
                    mode,
                    "--version",
                    "1",
                ],
            )
            .await,
        ] {
            assert_cancelled(&output, mode, &format!("knowledge entry {mode} in {space}"));
            assert_eq!(show_entry(&server, &space, mode).await, entry);
        }
        confirm_deletion(&server, &space, mode, &entry).await;
    }
}

#[tokio::test]
async fn knowledge_create_fails_with_server_error_when_caller_is_contributor() {
    let (server, space) = knowledge_context().await;
    let entry = create_entry(&server, &space, "guide.md", "Summary", "Content").await;
    server.login_as(SECOND_USERNAME).await;
    assert_success(
        &server
            .run(&[
                "member",
                "add",
                &space,
                SECOND_USERNAME,
                "--role",
                "contributor",
            ])
            .await,
    );
    for mode in ["human", "json"] {
        for mut args in [
            vec![
                "knowledge",
                "create",
                &space,
                "new.md",
                "--summary",
                "New",
                "--content",
                "New",
            ],
            vec![
                "knowledge",
                "change",
                &space,
                "KN-1",
                "--content",
                "Forbidden",
                "--version",
                "1",
            ],
            vec![
                "knowledge",
                "delete",
                &space,
                "guide.md",
                "--yes",
                "--version",
                "1",
            ],
        ] {
            args.extend(["--output", mode]);
            let output = server.run_as(SECOND_USERNAME, &args).await;
            assert_knowledge_error(&output, mode, "space.permission_required");
        }
    }
    assert_eq!(show_entry(&server, &space, "KN-1").await, entry);
    let list = server
        .run(&["knowledge", "list", &space, "--output", "json"])
        .await;
    assert_success(&list);
    assert_eq!(body(&list)["data"].as_array().unwrap().len(), 1);
    assert_viewer_reads_knowledge(&server, &space, &entry).await;
}

#[tokio::test]
async fn knowledge_create_reads_content_from_file() {
    let (server, space) = knowledge_context().await;
    let path = server.directory.path().join("content.md");
    let content = format!("\u{feff}{}", "# Guide\r\n\t\u{00e9} text\n".repeat(16_000));
    assert!(content.len() > 200 * 1024);
    fs::write(&path, content.as_bytes()).unwrap();
    let output = server
        .run(&[
            "knowledge",
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
    assert_eq!(body(&output)["data"]["content"], content);
    assert_eq!(
        show_entry(&server, &space, "KN-1").await["content"],
        content
    );
    assert_content_file_limits(&server, &space, &path).await;
    fs::write(&path, [0xff, 0xfe]).unwrap();
    for path in [
        path,
        server.directory.path().join("missing.md"),
        server.directory.path().to_path_buf(),
    ] {
        let output = server
            .run(&[
                "knowledge",
                "create",
                &space,
                "invalid.md",
                "--summary",
                "Invalid",
                "--content-file",
                path.to_str().unwrap(),
                "--output",
                "json",
            ])
            .await;
        assert_usage(&output);
    }
}

#[tokio::test]
async fn knowledge_change_reads_content_from_standard_input() {
    let (server, space) = knowledge_context().await;
    create_entry(&server, &space, "guide.md", "Keep summary", "Old").await;
    let content = "# Input\r\n\t\u{00e9} unchanged\n".repeat(10_000);
    let output = content_input(
        &server,
        &[
            "knowledge",
            "change",
            &space,
            "KN-1",
            "--content-file",
            "-",
            "--output",
            "json",
            "--version",
            "1",
        ],
        content.as_bytes(),
    )
    .await;
    assert_success(&output);
    let latest = show_entry(&server, &space, "guide.md").await;
    assert_eq!(body(&output)["data"], latest);
    assert_eq!(latest["content"], content);
    assert_eq!(latest["summary"], "Keep summary");
    assert_eq!(latest["version"], 2);
    let invalid = content_input(
        &server,
        &[
            "knowledge",
            "change",
            &space,
            "KN-1",
            "--content-file",
            "-",
            "--output",
            "json",
            "--version",
            "1",
        ],
        &[0xff],
    )
    .await;
    assert_usage(&invalid);
    assert_eq!(show_entry(&server, &space, "KN-1").await, latest);
    assert_other_content_sources(&server, &space).await;
}

#[tokio::test]
async fn knowledge_create_fails_with_usage_error_when_content_and_file_are_both_given() {
    let (mut server, space) = knowledge_context().await;
    server.stop();
    for mut arguments in [
        vec![
            "knowledge",
            "create",
            &space,
            "guide.md",
            "--summary",
            "Guide",
            "--content",
            "Text",
            "--content-file",
            "-",
        ],
        vec![
            "knowledge",
            "change",
            &space,
            "KN-1",
            "--content",
            "Text",
            "--content-file",
            "-",
            "--version",
            "1",
        ],
        vec![
            "knowledge",
            "create",
            &space,
            "guide.md",
            "--summary",
            "Guide",
        ],
        vec!["knowledge", "change", &space, "KN-1", "--version", "1"],
    ] {
        arguments.extend(["--output", "json"]);
        assert_usage(&server.run(&arguments).await);
    }
}

#[tokio::test]
async fn knowledge_show_reads_key_form_as_key() {
    let (server, space) = knowledge_context().await;
    let first = create_entry(&server, &space, "guide.md", "First", "First content").await;
    let second = create_entry(&server, &space, "KN-1", "Key-shaped name", "Second content").await;
    assert_eq!(show_entry(&server, &space, "KN-1").await, first);
    assert_eq!(show_entry(&server, &space, "KN-2").await, second);
    assert_eq!(show_entry(&server, &space, "kn-1").await, second);
    for name in [
        "KN-01",
        "KN-0",
        "KN-+1",
        "KN-1.md",
        "KN-1a",
        "KN-9223372036854775808",
    ] {
        let entry = create_entry(&server, &space, name, "Name", "Named content").await;
        assert_eq!(show_entry(&server, &space, name).await, entry);
    }
    assert_key_history(&server, &space).await;
    let changed = server
        .run(&[
            "knowledge",
            "change",
            &space,
            "KN-2",
            "--summary",
            "Updated",
            "--output",
            "json",
            "--version",
            "1",
        ])
        .await;
    assert_success(&changed);
    assert_eq!(show_entry(&server, &space, "KN-1").await, first);
    assert_eq!(
        show_entry(&server, &space, "KN-2").await["summary"],
        "Updated"
    );
    assert_success(
        &server
            .run(&[
                "knowledge",
                "delete",
                &space,
                "KN-01",
                "--yes",
                "--version",
                "2",
            ])
            .await,
    );
    assert_eq!(
        show_entry(&server, &space, "KN-01").await["kind"],
        "deletion"
    );
}

#[tokio::test]
async fn knowledge_show_prints_old_version() {
    let (server, space) = knowledge_context().await;
    let name = "guides/'quoted' ?#&%+\u{00e9}.md";
    let original = create_entry(
        &server,
        &space,
        name,
        "Old\n\t\u{1b}[31m",
        "# Old\n\t**raw**\r\u{1b}[31m",
    )
    .await;
    let changed = knowledge_json(
        &server,
        &[
            "change",
            &space,
            name,
            "--summary",
            "New",
            "--content",
            "New content",
        ],
    )
    .await;
    for entry in [name, "KN-1"] {
        assert_eq!(
            read_entry_version(&server, &space, entry, 1).await,
            original
        );
        assert_eq!(show_entry(&server, &space, entry).await, changed);
    }
    let old = server
        .run(&["knowledge", "show", &space, name, "--version", "1"])
        .await;
    assert_success(&old);
    assert!(old.stderr.is_empty());
    let text = String::from_utf8(old.stdout).unwrap();
    assert!(text.contains("Summary: Old\\n\\t\\u{1b}[31m\n"));
    assert!(text.contains("Content: # Old\n\t**raw**\\r\\u{1b}[31m\n"));
    assert!(!text.contains('\u{1b}'));
    let deleted = knowledge_json(&server, &["delete", &space, name, "--yes"]).await;
    assert_eq!(deleted["version"], 3);
    for entry in [name, "KN-1"] {
        assert_eq!(read_entry_version(&server, &space, entry, 3).await, deleted);
        assert_eq!(
            read_entry_version(&server, &space, entry, 1).await,
            original
        );
    }
    let deletion = server
        .run(&["knowledge", "show", &space, "KN-1", "--version", "3"])
        .await;
    assert_success(&deletion);
    let text = String::from_utf8(deletion.stdout).unwrap();
    assert!(text.contains("Kind: deletion\nSummary: \nContent: \n"));
    assert!(text.contains(&format!(
        "Applied by: {}\nApplied at: {}\n",
        server.username, deleted["applied_at"]
    )));
    assert_history_pages(&server, &space, name, &[original, changed, deleted]).await;
    assert_history_errors(&server, &space).await;
}

#[tokio::test]
async fn knowledge_deleted_lists_deleted_entry() {
    let (server, space) = knowledge_context().await;
    let names = ["first.md", "KN-99", "last.md"];
    let mut expected = Vec::new();
    for name in names {
        let entry = create_entry(&server, &space, name, "Delete", "Old content").await;
        let key = entry["key"].as_str().unwrap();
        let deleted = knowledge_json(&server, &["delete", &space, key, "--yes"]).await;
        expected.push(json!({"name": name, "key": key, "deleted_by": deleted["applied_by"], "deleted_at": deleted["applied_at"]}));
    }
    let live = create_entry(&server, &space, "live.md", "Keep", "Live content").await;
    let items = pages(&server, &["knowledge", "deleted", &space], "name", &names).await;
    assert_eq!(items, expected);
    let human = server.run(&["knowledge", "deleted", &space]).await;
    assert_success(&human);
    assert!(human.stderr.is_empty());
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(text.starts_with("Name\tKey\tDeleted by\tDeleted at\n"));
    for item in &expected {
        assert!(text.contains(&format!(
            "{}\t{}\t{}\t{}\n",
            item["name"].as_str().unwrap(),
            item["key"].as_str().unwrap(),
            server.username,
            item["deleted_at"]
        )));
        let entry = read_entry_version(&server, &space, item["key"].as_str().unwrap(), 1).await;
        assert_eq!(entry["content"], "Old content");
    }
    assert!(!text.contains("live.md"));
    assert_eq!(show_entry(&server, &space, "live.md").await, live);
}

#[tokio::test]
async fn knowledge_rollback_restores_old_content() {
    let (server, space) = knowledge_context().await;
    let original = create_entry(
        &server,
        &space,
        "guide.md",
        "First summary",
        "# First\n\ttext",
    )
    .await;
    let name = "guides/renamed ?#&%+\u{00e9}.md";
    let changed = knowledge_json(
        &server,
        &[
            "change",
            &space,
            "KN-1",
            "--name",
            name,
            "--summary",
            "Second summary",
            "--content",
            "# Second\n\ttext",
        ],
    )
    .await;
    let deleted = knowledge_json(&server, &["delete", &space, "KN-1", "--yes"]).await;
    assert_rollback_refusals(&server, &space, &deleted).await;
    let restored = knowledge_json(&server, &["rollback", &space, name, "1"]).await;
    assert_rollback_content(&restored, &original, name, 4);
    assert_eq!(show_entry(&server, &space, "KN-1").await, restored);
    assert_eq!(
        knowledge_json(&server, &["deleted", &space]).await,
        json!([])
    );
    rollback_second_version(&server, &space).await;
    let latest = show_entry(&server, &space, "KN-1").await;
    assert_rollback_content(&latest, &changed, name, 5);
    let found = knowledge_json(&server, &["list", &space, "--keyword", "Second summary"]).await;
    assert_eq!(
        found,
        json!([{"name": name, "key": "KN-1", "summary": "Second summary"}])
    );
    assert_history_pages(
        &server,
        &space,
        "KN-1",
        &[original, changed, deleted, restored, latest],
    )
    .await;
}

#[tokio::test]
async fn knowledge_rollback_fails_with_server_error_when_caller_is_contributor() {
    let (mut server, space) = knowledge_context().await;
    let original = create_entry(&server, &space, "guide.md", "Summary", "Keep content").await;
    server.login_as(SECOND_USERNAME).await;
    assert_success(
        &server
            .run(&[
                "member",
                "add",
                &space,
                SECOND_USERNAME,
                "--role",
                "contributor",
            ])
            .await,
    );
    for mode in ["human", "json"] {
        for entry in ["guide.md", "KN-1"] {
            let output = server
                .run_as(
                    SECOND_USERNAME,
                    &[
                        "knowledge",
                        "rollback",
                        &space,
                        entry,
                        "1",
                        "--output",
                        mode,
                        "--version",
                        "1",
                    ],
                )
                .await;
            assert_knowledge_error(&output, mode, "space.permission_required");
        }
    }
    assert_eq!(show_entry(&server, &space, "KN-1").await, original);
    let history = knowledge_json(&server, &["history", &space, "KN-1"]).await;
    assert_eq!(history.as_array().unwrap().len(), 1);
    assert_eq!(history[0]["version"], 1);
    assert_viewer_reads_history(&server, &space, &original).await;
    assert_version_usage(&mut server, &space).await;
}

#[tokio::test]
async fn knowledge_reads_preserve_server_errors() {
    let (server, space) = knowledge_context().await;
    create_entry(&server, &space, "guide.md", "Summary", "Keep content").await;
    for mode in ["human", "json"] {
        for (mut arguments, code) in [
            (
                vec!["knowledge", "show", &space, "KN-99"],
                "knowledge.not_found",
            ),
            (
                vec!["knowledge", "show", &space, "missing ?#&%+.md"],
                "knowledge.not_found",
            ),
            (
                vec![
                    "knowledge",
                    "show",
                    "SPACE-1/../SPACE-1?query#fragment%",
                    "KN-1",
                ],
                "space.not_found",
            ),
            (
                vec!["knowledge", "list", &space, "--page-size", "0"],
                "request.invalid_query",
            ),
            (
                vec!["knowledge", "list", &space, "--cursor", "invalid"],
                "request.invalid_query",
            ),
            (
                vec![
                    "knowledge",
                    "create",
                    &space,
                    "guide.md",
                    "--summary",
                    "Duplicate",
                    "--content",
                    "Duplicate",
                ],
                "knowledge.name_taken",
            ),
        ] {
            arguments.extend(["--output", mode]);
            assert_knowledge_error(&server.run(&arguments).await, mode, code);
        }
    }
    assert_eq!(
        show_entry(&server, &space, "KN-1").await["content"],
        "Keep content"
    );
}
