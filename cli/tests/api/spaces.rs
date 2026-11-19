use serde_json::{Value, json};

use crate::helpers::{
    SECOND_USERNAME, assert_participant, assert_removal, assert_removed, assert_success,
    assert_usage, body, create, pages, team,
};

#[tokio::test]
async fn participant_add_lets_second_person_list_space() {
    let server = team().await;
    let space = create(&server, "Team", "Team work").await;
    for mode in ["human", "json"] {
        let users = server.run(&["user", "list", "--output", mode]).await;
        assert_success(&users);
        if mode == "json" {
            assert_eq!(body(&users)["data"].as_array().unwrap().len(), 2);
            assert!(
                body(&users)["data"]
                    .as_array()
                    .unwrap()
                    .contains(&json!({"username": SECOND_USERNAME}))
            );
        } else {
            assert!(String::from_utf8_lossy(&users.stdout).contains(SECOND_USERNAME));
        }
    }
    let added = server
        .run(&[
            "participant",
            "add",
            &space,
            SECOND_USERNAME,
            "--role",
            "viewer",
            "--output",
            "json",
        ])
        .await;
    assert_success(&added);
    assert_eq!(
        body(&added),
        json!({"data": {"type": "human", "username": SECOND_USERNAME, "role": "viewer"}, "meta": {}})
    );
    for mode in ["human", "json"] {
        let list = server
            .run_as(SECOND_USERNAME, &["space", "list", "--output", mode])
            .await;
        assert_success(&list);
        assert!(list.stderr.is_empty());
        if mode == "json" {
            assert_eq!(
                body(&list),
                json!({"data": [{"key": space, "name": "Team", "summary": "Team work", "role": "viewer", "caller_person": SECOND_USERNAME}], "meta": {}})
            );
        } else {
            assert_eq!(
                String::from_utf8(list.stdout).unwrap(),
                format!("Key\tName\tSummary\tRole\n{space}\tTeam\tTeam work\tviewer\n")
            );
        }
    }
}

#[tokio::test]
async fn space_rename_updates_name_and_summary() {
    let server = team().await;
    let space = create(&server, "Team", "Team work").await;
    let changed = server
        .run(&[
            "space",
            "change",
            &space,
            "--name",
            "Renamed",
            "--summary",
            "New summary",
            "--output",
            "json",
        ])
        .await;
    assert_success(&changed);
    let expected = json!({"data": {"key": space, "name": "Renamed", "summary": "New summary", "role": "owner", "caller_person": server.username}, "meta": {}});
    assert_eq!(body(&changed), expected);
    let shown = server
        .run(&["space", "show", &space, "--output", "json"])
        .await;
    assert_success(&shown);
    assert_eq!(body(&shown), expected);
    let human = server.run(&["space", "show", &space]).await;
    assert_success(&human);
    assert_eq!(
        String::from_utf8(human.stdout).unwrap(),
        format!("Key: {space}\nName: Renamed\nSummary: New summary\nRole: owner\n")
    );
    for arguments in [vec!["--name", "Final"], vec!["--summary", "Final summary"]] {
        let mut command = vec!["space", "change", &space];
        command.extend(arguments);
        assert_success(&server.run(&command).await);
    }
    let final_space = server
        .run(&["space", "show", &space, "--output", "json"])
        .await;
    assert_success(&final_space);
    assert_eq!(body(&final_space)["data"]["name"], "Final");
    assert_eq!(body(&final_space)["data"]["summary"], "Final summary");
}

#[tokio::test]
async fn participant_set_role_changes_role_in_participant_list() {
    let server = team().await;
    let space = create(&server, "Team", "Team work").await;
    for mode in ["human", "json"] {
        assert_success(
            &server
                .run(&[
                    "participant",
                    "add",
                    &space,
                    SECOND_USERNAME,
                    "--role",
                    "viewer",
                    "--output",
                    mode,
                ])
                .await,
        );
        for role in ["contributor", "admin", "viewer"] {
            let changed = server
                .run(&[
                    "participant",
                    "set-role",
                    &space,
                    SECOND_USERNAME,
                    "--role",
                    role,
                    "--output",
                    mode,
                ])
                .await;
            assert_participant(&changed, mode, role);
            let roster = server
                .run(&["participant", "list", &space, "--output", "json"])
                .await;
            assert_success(&roster);
            assert!(
                body(&roster)["data"]
                    .as_array()
                    .unwrap()
                    .contains(&json!({"type": "human", "username": SECOND_USERNAME, "role": role}))
            );
        }
        let removed = server
            .run(&[
                "participant",
                "remove",
                &space,
                SECOND_USERNAME,
                "--output",
                mode,
            ])
            .await;
        assert_removal(&removed, mode);
        assert_removed(&server, &space).await;
    }
}

#[tokio::test]
async fn participant_add_fails_with_server_error_when_caller_is_contributor() {
    let server = team().await;
    let space = create(&server, "Team", "Team work").await;
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
        let refused = server
            .run_as(
                SECOND_USERNAME,
                &[
                    "participant",
                    "add",
                    &space,
                    &server.username,
                    "--role",
                    "viewer",
                    "--output",
                    mode,
                ],
            )
            .await;
        assert_eq!(refused.status.code(), Some(1));
        assert!(refused.stdout.is_empty());
        assert_eq!(String::from_utf8_lossy(&refused.stderr).lines().count(), 1);
        if mode == "json" {
            let error: Value = serde_json::from_slice(&refused.stderr).unwrap();
            assert_eq!(error["status"], 403);
            assert_eq!(error["code"], "space.permission_required");
        } else {
            assert_eq!(
                refused.stderr,
                b"The space role does not allow this operation\n"
            );
        }
    }
    let roster = server
        .run(&["participant", "list", &space, "--output", "json"])
        .await;
    assert_success(&roster);
    assert_eq!(body(&roster)["data"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn lists_return_one_page_and_preserve_roster_order() {
    let server = team().await;
    let first = create(&server, "First", "First summary").await;
    let second = create(&server, "Second", "Second summary").await;
    let third = create(&server, "Third", "Third summary").await;
    server.agent("Beta").await;
    server.agent("Alpha").await;
    assert_success(
        &server
            .run(&[
                "participant",
                "add",
                &first,
                SECOND_USERNAME,
                "--role",
                "viewer",
            ])
            .await,
    );
    pages(
        &server,
        &["space", "list"],
        "key",
        &[&first, &second, &third],
    )
    .await;
    pages(
        &server,
        &["user", "list"],
        "username",
        &[&server.username, SECOND_USERNAME],
    )
    .await;
    let roster = pages(
        &server,
        &["participant", "list", &first],
        "username",
        &[
            &server.username,
            &server.username,
            &server.username,
            SECOND_USERNAME,
        ],
    )
    .await;
    assert_eq!(roster[0]["type"], "human");
    assert_eq!(
        roster[1],
        json!({"type": "agent", "username": server.username, "agent_name": "Alpha", "role": "contributor"})
    );
    assert_eq!(roster[2]["agent_name"], "Beta");
    assert_eq!(roster[3]["type"], "human");
}

#[tokio::test]
async fn space_output_preserves_multiline_text_and_escapes_terminal_controls() {
    let server = team().await;
    let name = "Team\u{1b}[31m\n\t\r\u{7}";
    let summary = "Line one\nLine two\tTab\u{1b}[0m\r";
    let created = server
        .run(&["space", "create", "--name", name, "--summary", summary])
        .await;
    assert_success(&created);
    let human = String::from_utf8(created.stdout).unwrap();
    assert!(human.contains("Name: Team\\u{1b}[31m\\n\\t\\r\\u{7}"));
    assert!(human.contains("Summary: Line one\nLine two\tTab\\u{1b}[0m\\r"));
    let list = server.run(&["space", "list"]).await;
    assert_success(&list);
    let list = String::from_utf8(list.stdout).unwrap();
    assert_eq!(list.lines().count(), 2);
    assert!(list.contains("Line one\\nLine two\\tTab\\u{1b}[0m\\r"));
    let shown = server
        .run(&["space", "show", "SPACE-1", "--output", "json"])
        .await;
    assert_success(&shown);
    assert_eq!(body(&shown)["data"]["name"], name);
    assert_eq!(body(&shown)["data"]["summary"], summary);
}

#[tokio::test]
async fn commands_reject_invalid_arguments_before_calling_server() {
    let mut server = team().await;
    let space = create(&server, "Team", "Team work").await;
    server.stop();
    let commands = [
        vec!["space", "change", &space],
        vec!["space", "create", "--name", "Missing summary"],
        vec![
            "participant",
            "add",
            &space,
            SECOND_USERNAME,
            "--role",
            "admin",
        ],
        vec![
            "participant",
            "add",
            &space,
            SECOND_USERNAME,
            "--role",
            "owner",
        ],
        vec![
            "participant",
            "add",
            &space,
            SECOND_USERNAME,
            "--role",
            "vewer",
        ],
        vec![
            "participant",
            "set-role",
            &space,
            SECOND_USERNAME,
            "--role",
            "owner",
        ],
        vec![
            "participant",
            "set-role",
            &space,
            SECOND_USERNAME,
            "--role",
            "vewer",
        ],
        vec!["participant", "add", &space, SECOND_USERNAME],
    ];
    for mut arguments in commands {
        arguments.extend(["--output", "json"]);
        let output = server.run(&arguments).await;
        assert_usage(&output);
    }
    for arguments in [
        vec!["space", "--help"],
        vec!["participant", "set-role", "--help"],
        vec!["user", "list", "--help"],
    ] {
        assert_success(&server.run(&arguments).await);
    }
}

#[tokio::test]
async fn commands_preserve_server_validation_and_hidden_space_errors() {
    let server = team().await;
    let space = create(&server, "Team", "Team work").await;
    for mode in ["human", "json"] {
        for arguments in [
            vec!["space", "create", "--name", "", "--summary", "Summary"],
            vec!["space", "list", "--page-size", "0"],
            vec!["user", "list", "--cursor", "invalid"],
            vec!["participant", "list", &space, "--page-size", "0"],
        ] {
            let mut arguments = arguments;
            arguments.extend(["--output", mode]);
            let output = server.run(&arguments).await;
            assert_eq!(output.status.code(), Some(1));
            assert!(output.stdout.is_empty());
            assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
            if mode == "json" {
                let error: Value = serde_json::from_slice(&output.stderr).unwrap();
                assert_eq!(error["status"], 400);
                assert!(!error["errors"].as_array().unwrap().is_empty());
            } else {
                assert!(String::from_utf8_lossy(&output.stderr).starts_with("Bad Request: "));
            }
        }
        let hidden = server
            .run_as(
                SECOND_USERNAME,
                &["space", "show", &space, "--output", mode],
            )
            .await;
        assert_eq!(hidden.status.code(), Some(1));
        assert!(hidden.stdout.is_empty());
        if mode == "json" {
            let error: Value = serde_json::from_slice(&hidden.stderr).unwrap();
            assert_eq!(error["status"], 404);
        } else {
            assert_eq!(hidden.stderr, b"Space not found\n");
        }
    }
}
