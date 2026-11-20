use std::fs;

use serde_json::json;

use crate::helpers::{
    SECOND_USERNAME, assert_cancelled, assert_content_file_limits, assert_empty_knowledge_search,
    assert_knowledge_error, assert_knowledge_prefix_pages, assert_other_content_sources,
    assert_success, assert_usage, assert_viewer_reads_knowledge, body, content_input, create_entry,
    knowledge_context, pages, piped_yes, show_entry,
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
async fn knowledge_change_updates_latest_content_and_version() {
    let (server, space) = knowledge_context().await;
    create_entry(&server, &space, "guide.md", "Original", "Old content").await;
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
        let changed = server
            .run(&[
                "knowledge",
                "change",
                &space,
                "guide.md",
                option,
                value,
                "--output",
                "json",
            ])
            .await;
        assert_success(&changed);
        assert!(changed.stderr.is_empty());
        let latest = show_entry(&server, &space, "KN-1").await;
        assert_eq!(body(&changed), json!({"data": latest, "meta": {}}));
        assert_eq!(latest["version"], version);
        assert_eq!(latest["summary"], summary);
        assert_eq!(latest["content"], content);
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
        ])
        .await;
    assert_success(&changed);
    assert!(
        String::from_utf8(changed.stdout)
            .unwrap()
            .contains("Content: # Human\n\tcontent\n")
    );
    assert_eq!(show_entry(&server, &space, "KN-1").await["version"], 4);
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
                .run(&["knowledge", "delete", &space, mode, "--output", mode])
                .await,
            piped_yes(
                &server,
                &["knowledge", "delete", &space, mode, "--output", mode],
            )
            .await,
        ] {
            assert_cancelled(&output, mode, &format!("knowledge entry {mode} in {space}"));
            assert_eq!(show_entry(&server, &space, mode).await, entry);
        }
        let key = entry["key"].as_str().unwrap();
        let deleted = server
            .run(&[
                "knowledge",
                "delete",
                &space,
                key,
                "--yes",
                "--output",
                mode,
            ])
            .await;
        assert_success(&deleted);
        assert!(deleted.stderr.is_empty());
        let latest = show_entry(&server, &space, key).await;
        assert_eq!(latest["kind"], "deletion");
        assert_eq!(latest["version"], 2);
        assert_eq!(latest["content"], json!(null));
        assert_eq!(latest["summary"], json!(null));
        assert_eq!(latest["applied_by"], server.username);
        if mode == "json" {
            assert_eq!(body(&deleted), json!({"data": latest, "meta": {}}));
        } else {
            assert_eq!(deleted.stdout, b"Knowledge entry deleted.\n");
        }
        let shown = server.run(&["knowledge", "show", &space, key]).await;
        assert_success(&shown);
        assert!(
            String::from_utf8(shown.stdout)
                .unwrap()
                .contains("Kind: deletion\n")
        );
        let list = server
            .run(&["knowledge", "list", &space, "--output", "json"])
            .await;
        assert_success(&list);
        assert_eq!(body(&list), json!({"data": [], "meta": {}}));
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
                "participant",
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
            ],
            vec!["knowledge", "delete", &space, "guide.md", "--yes"],
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
        ],
        vec![
            "knowledge",
            "create",
            &space,
            "guide.md",
            "--summary",
            "Guide",
        ],
        vec!["knowledge", "change", &space, "KN-1"],
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
            .run(&["knowledge", "delete", &space, "KN-01", "--yes"])
            .await,
    );
    assert_eq!(
        show_entry(&server, &space, "KN-01").await["kind"],
        "deletion"
    );
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
