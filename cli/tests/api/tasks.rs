use serde_json::json;

use crate::helpers::{
    SECOND_USERNAME, append_note, assert_success, assert_task_error, body, create, create_task,
    read_task, task_context, task_id_from_show, task_pages, task_with_artifacts,
};

#[tokio::test]
async fn task_list_returns_tasks_in_activity_order() {
    let (server, space, agent) = task_context().await;
    let title = "**First**\nLine\tTab\u{1b}[0m\u{202e}";
    let first = create_task(&server, &space, &agent, title).await;
    create_task(&server, &space, &agent, "Second").await;
    create_task(&server, &space, &agent, "Third").await;
    let time = append_note(&server, &space, &first, &agent, "Newest activity").await;
    for mode in ["human", "json"] {
        let (first, last, cursor) = task_pages(&server, &["task", "list", &space], mode).await;
        if mode == "json" {
            assert_eq!(body(&first)["data"].as_array().unwrap().len(), 2);
            assert_eq!(body(&first)["data"][0]["key"], "TK-1");
            assert_eq!(body(&first)["data"][0]["title"], title);
            assert_eq!(body(&first)["data"][0]["last_activity_at"], time);
            assert_eq!(body(&first)["data"][1]["key"], "TK-3");
            assert_eq!(body(&first)["meta"]["next_cursor"], cursor);
            assert_eq!(body(&last)["data"].as_array().unwrap().len(), 1);
            assert_eq!(body(&last)["data"][0]["key"], "TK-2");
            assert_eq!(body(&last)["meta"], json!({}));
        } else {
            let text = String::from_utf8(first.stdout).unwrap();
            assert_eq!(text.lines().count(), 3);
            assert!(text.contains(&format!("TK-1\t**First**\\nLine\\tTab\\u{{1b}}[0m\\u{{202e}}\tin progress\t{}\tTask agent\t{time}\n", server.username)));
            assert!(!text.contains("Second"));
            assert!(String::from_utf8_lossy(&last.stdout).contains("TK-2\tSecond\t"));
        }
    }
    let empty = create(&server, "Empty", "No tasks").await;
    let list = server
        .run(&["task", "list", &empty, "--output", "json"])
        .await;
    assert_success(&list);
    assert_eq!(body(&list), json!({"data": [], "meta": {}}));
    assert!(list.stderr.is_empty());
}

#[tokio::test]
async fn task_show_prints_result_task_id_and_artifacts() {
    let (server, space, agent) = task_context().await;
    let title = "# Work\n- source\ttext\u{1b}[0m\r\u{7}\u{2066}";
    let result = "# Findings\n- **Done**\tcode\u{1b}[0m\r\u{7}\u{202e}";
    let expected = task_with_artifacts(&server, &space, &agent, title, result).await;
    let shown = server
        .run(&["task", "show", &space, "TK-1", "--output", "json"])
        .await;
    assert_success(&shown);
    assert!(shown.stderr.is_empty());
    assert_eq!(body(&shown), json!({"data": expected, "meta": {}}));
    assert_eq!(
        body(&shown)["data"]["artifacts"].as_array().unwrap().len(),
        2
    );
    let human = server.run(&["task", "show", &space, "TK-1"]).await;
    assert_success(&human);
    assert!(human.stderr.is_empty());
    let id = expected["task_id"].as_str().unwrap();
    let started = expected["started_at"].as_i64().unwrap();
    let last_activity = expected["last_activity_at"].as_i64().unwrap();
    let artifacts = &expected["artifacts"];
    assert_eq!(
        String::from_utf8(human.stdout).unwrap(),
        format!(
            "Key: TK-1\nTask ID: {id}\nTitle: # Work\n- source\ttext\\u{{1b}}[0m\\r\\u{{7}}\\u{{2066}}\nStatus: in progress\nPerson: {}\nAgent name: Task agent\nStarted at: {started}\nEnded at: \nLast activity at: {last_activity}\nResult: # Findings\n- **Done**\tcode\\u{{1b}}[0m\\r\\u{{7}}\\u{{202e}}\nArtifacts: {artifacts}\n",
            server.username
        )
    );
}

#[tokio::test]
async fn task_close_ends_task_named_by_task_id() {
    let (server, space, agent) = task_context().await;
    for mode in ["human", "json"] {
        let mut expected = task_with_artifacts(
            &server,
            &space,
            &agent,
            "Keep title",
            "**Keep result**\nDetails",
        )
        .await;
        let key = expected["key"].as_str().unwrap().to_owned();
        let id = task_id_from_show(&server, &space, &key, mode).await;
        assert_eq!(expected["task_id"], id);
        let closed = server
            .run(&["task", "close", &space, &id, "--output", mode])
            .await;
        assert_success(&closed);
        assert!(closed.stderr.is_empty());
        let after = read_task(&server, &space, &key).await;
        let ended = after["ended_at"].as_i64().unwrap();
        assert!(ended >= expected["started_at"].as_i64().unwrap());
        expected["status"] = json!("ended");
        expected["ended_at"] = json!(ended);
        assert_eq!(after, expected);
        if mode == "json" {
            assert_eq!(body(&closed), json!({"data": expected, "meta": {}}));
        } else {
            let shown = server.run(&["task", "show", &space, &key]).await;
            assert_success(&shown);
            assert_eq!(closed.stdout, shown.stdout);
            let text = String::from_utf8(closed.stdout).unwrap();
            assert!(text.contains("Status: ended\n"));
            assert!(text.contains(&format!("Ended at: {ended}\n")));
        }
    }
}

#[tokio::test]
async fn task_close_fails_with_server_error_when_given_task_key() {
    let (server, space, agent) = task_context().await;
    let task = create_task(&server, &space, &agent, "Keep task").await;
    let key = task["key"].as_str().unwrap();
    let expected = read_task(&server, &space, key).await;
    assert_eq!(expected["status"], "in progress");
    for mode in ["human", "json"] {
        for argument in [key, "task/id?query#fragment%"] {
            let output = server
                .run(&["task", "close", &space, argument, "--output", mode])
                .await;
            assert_task_error(&output, mode, "task.not_found");
            if mode == "human" {
                assert_eq!(output.stderr, b"Task not found\n");
            }
            assert_eq!(read_task(&server, &space, key).await, expected);
        }
    }
}

#[tokio::test]
async fn task_scratchpad_prints_notes_in_time_order() {
    let (server, space, agent) = task_context().await;
    let task = create_task(&server, &space, &agent, "Notes").await;
    let contents = [
        "**First**\n- detail\tcode\u{1b}[31m\r\u{7}\u{202e}",
        "Second",
        "Third",
    ];
    let mut times = Vec::new();
    for content in contents {
        times.push(append_note(&server, &space, &task, &agent, content).await);
    }
    assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
    for mode in ["human", "json"] {
        let (first, last, cursor) =
            task_pages(&server, &["task", "scratchpad", &space, "TK-1"], mode).await;
        if mode == "json" {
            assert_eq!(
                body(&first)["data"],
                json!([
                    {"time": times[0], "content": contents[0]},
                    {"time": times[1], "content": contents[1]}
                ])
            );
            assert_eq!(body(&first)["meta"]["next_cursor"], cursor);
            assert_eq!(
                body(&last),
                json!({"data": [{"time": times[2], "content": contents[2]}], "meta": {}})
            );
        } else {
            assert_eq!(
                String::from_utf8(first.stdout).unwrap(),
                format!(
                    "Time\tContent\n{}\t**First**\n- detail\tcode\\u{{1b}}[31m\\r\\u{{7}}\\u{{202e}}\n{}\tSecond\n",
                    times[0], times[1]
                )
            );
            assert_eq!(
                String::from_utf8(last.stdout).unwrap(),
                format!("Time\tContent\n{}\tThird\n", times[2])
            );
        }
    }
    create_task(&server, &space, &agent, "Empty notes").await;
    let empty = server
        .run(&["task", "scratchpad", &space, "TK-2", "--output", "json"])
        .await;
    assert_success(&empty);
    assert_eq!(body(&empty), json!({"data": [], "meta": {}}));
    assert!(empty.stderr.is_empty());
}

#[tokio::test]
async fn task_reads_preserve_server_errors_and_encoded_path_segments() {
    let (server, space, agent) = task_context().await;
    create_task(&server, &space, &agent, "Keep task").await;
    server.login_as(SECOND_USERNAME).await;
    for mode in ["human", "json"] {
        for (arguments, code) in [
            (vec!["task", "show", &space, "TK-99"], "task.not_found"),
            (
                vec!["task", "show", &space, "TK-1/../TK-1?query#fragment%"],
                "task.not_found",
            ),
            (
                vec!["task", "scratchpad", &space, "TK-1/../TK-1?query#fragment%"],
                "task.not_found",
            ),
            (
                vec!["task", "list", "SPACE-1/../SPACE-1?query#fragment%"],
                "space.not_found",
            ),
            (
                vec!["task", "list", &space, "--page-size", "0"],
                "request.invalid_query",
            ),
            (
                vec!["task", "scratchpad", &space, "TK-1", "--cursor", "invalid"],
                "request.invalid_query",
            ),
        ] {
            let mut arguments = arguments;
            arguments.extend(["--output", mode]);
            assert_task_error(&server.run(&arguments).await, mode, code);
        }
        for mut arguments in [
            vec!["task", "list", &space],
            vec!["task", "show", &space, "TK-1"],
            vec!["task", "scratchpad", &space, "TK-1"],
        ] {
            arguments.extend(["--output", mode]);
            assert_task_error(
                &server.run_as(SECOND_USERNAME, &arguments).await,
                mode,
                "space.not_found",
            );
        }
    }
    let shown = server
        .run(&["task", "show", &space, "TK-1", "--output", "json"])
        .await;
    assert_success(&shown);
    assert_eq!(body(&shown)["data"]["title"], "Keep task");
}
