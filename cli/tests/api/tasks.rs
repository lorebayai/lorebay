use serde_json::json;

use crate::helpers::{
    SECOND_USERNAME, Server, append_checkpoint, assert_success, assert_task_error, assert_usage,
    assert_waiting_task_listed, body, create, create_task, read_task, task_context,
    task_id_from_show, task_pages, task_with_agent_files,
};

#[tokio::test]
async fn task_list_returns_tasks_in_activity_order() {
    let (server, space, agent) = task_context().await;
    let title = "**First**\nLine\tTab\u{1b}[0m\u{202e}";
    let first = create_task(&server, &space, &agent, title).await;
    create_task(&server, &space, &agent, "Second").await;
    create_task(&server, &space, &agent, "Third").await;
    let time = append_checkpoint(&server, &space, &first, &agent, "Newest activity").await;
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
            assert!(text.contains(&format!("TK-1\t**First**\\nLine\\tTab\\u{{1b}}[0m\\u{{202e}}\tworking\t{}\tTask agent\tTask agent\t\t\t\t{time}\n", server.username)));
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
async fn task_show_prints_result_task_id_and_agent_files() {
    let (server, space, agent) = task_context().await;
    let title = "# Work\nState: canceled\n- source\ttext\u{1b}[0m\r\u{7}\u{2066}";
    let result = "# Findings\nTask ID: another-task\n\n- **Done**\tcode\u{1b}[0m\r\u{7}\u{202e}\n";
    let expected = task_with_agent_files(&server, &space, &agent, title, result).await;
    let shown = server
        .run(&["task", "show", &space, "TK-1", "--output", "json"])
        .await;
    assert_success(&shown);
    assert!(shown.stderr.is_empty());
    assert_eq!(body(&shown), json!({"data": expected, "meta": {}}));
    assert_eq!(body(&shown)["data"]["files"].as_array().unwrap().len(), 2);
    let human = server.run(&["task", "show", &space, "TK-1"]).await;
    assert_success(&human);
    assert!(human.stderr.is_empty());
    let id = expected["task_id"].as_str().unwrap();
    let started = expected["started_at"].as_i64().unwrap();
    let last_activity = expected["last_activity_at"].as_i64().unwrap();
    let agent_files = &expected["files"];
    assert_eq!(
        String::from_utf8(human.stdout).unwrap(),
        format!(
            "Key: TK-1\nTask ID: {id}\nTitle: # Work\n  State: canceled\n  - source\ttext\\u{{1b}}[0m\\r\\u{{7}}\\u{{2066}}\nState: working\nPerson: {}\nAgent name: Task agent\nHolder: Task agent\nReason: \nPlanner: \nPlanner agent: \nCreated at: {started}\nStarted at: {started}\nEnded at: \nLast activity at: {last_activity}\nResult: # Findings\n  Task ID: another-task\n  \n  - **Done**\tcode\\u{{1b}}[0m\\r\\u{{7}}\\u{{202e}}\n  \nFiles: {agent_files}\n",
            server.username
        )
    );
}

#[tokio::test]
async fn task_cancel_cancels_task_named_by_task_id() {
    let (server, space, agent) = task_context().await;
    for mode in ["human", "json"] {
        let mut expected = task_with_agent_files(
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
            .run(&[
                "task", "cancel", &space, &id, "--reason", "Stopped", "--output", mode,
            ])
            .await;
        assert_success(&closed);
        assert!(closed.stderr.is_empty());
        let after = read_task(&server, &space, &key).await;
        let ended = after["ended_at"].as_i64().unwrap();
        assert!(ended >= expected["started_at"].as_i64().unwrap());
        expected["state"] = json!("canceled");
        expected["state_reason"] = json!("Stopped");
        expected["holder"] = serde_json::Value::Null;
        expected["ended_at"] = json!(ended);
        assert_eq!(after, expected);
        if mode == "json" {
            assert_eq!(body(&closed), json!({"data": expected, "meta": {}}));
        } else {
            let shown = server.run(&["task", "show", &space, &key]).await;
            assert_success(&shown);
            assert_eq!(closed.stdout, shown.stdout);
            let text = String::from_utf8(closed.stdout).unwrap();
            assert!(text.contains("State: canceled\n"));
            assert!(text.contains(&format!("Ended at: {ended}\n")));
        }
    }
}

#[tokio::test]
async fn task_cancel_fails_with_server_error_when_given_task_key() {
    let (server, space, agent) = task_context().await;
    let task = create_task(&server, &space, &agent, "Keep task").await;
    let key = task["key"].as_str().unwrap();
    let expected = read_task(&server, &space, key).await;
    assert_eq!(expected["state"], "working");
    for mode in ["human", "json"] {
        for argument in [key, "task/id?query#fragment%"] {
            let output = server
                .run(&[
                    "task", "cancel", &space, argument, "--reason", "Stopped", "--output", mode,
                ])
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
async fn task_scratchpad_prints_checkpoints_in_time_order() {
    let (server, space, agent) = task_context().await;
    let task = create_task(&server, &space, &agent, "Checkpoints").await;
    let contents = [
        "**First**\n- detail\tcode\u{1b}[31m\r\u{7}\u{202e}",
        "Second",
        "Third",
    ];
    let mut times = Vec::new();
    for content in contents {
        times.push(append_checkpoint(&server, &space, &task, &agent, content).await);
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
    create_task(&server, &space, &agent, "Empty checkpoints").await;
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

#[tokio::test]
async fn task_plan_lists_queue_and_cancel_requires_reason() {
    let (server, space, _) = task_context().await;
    let planned = server
        .run(&[
            "task",
            "plan",
            &space,
            "--title",
            "Queued work",
            "--output",
            "json",
        ])
        .await;
    assert_success(&planned);
    assert_eq!(body(&planned)["data"]["title"], "Queued work");
    let id = body(&planned)["data"]["task_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let shown = read_task(&server, &space, "TK-1").await;
    assert_eq!(shown["state"], "planned");
    assert_eq!(shown["planner"], server.username);
    assert!(shown["holder"].is_null());
    for filters in [vec!["--state", "planned"], vec!["--claimable"]] {
        let mut args = vec!["task", "list", &space, "--output", "json"];
        args.extend(filters);
        let listed = server.run(&args).await;
        assert_success(&listed);
        assert_eq!(body(&listed)["data"][0]["key"], "TK-1");
    }
    let open = server
        .run(&["task", "list", &space, "--own-open", "--output", "json"])
        .await;
    assert_success(&open);
    assert_eq!(body(&open)["data"], json!([]));
    assert_waiting_task_listed(&server, &space).await;
    let missing = server
        .run(&["task", "cancel", &space, &id, "--output", "json"])
        .await;
    assert_usage(&missing);
    let canceled = server
        .run(&[
            "task",
            "cancel",
            &space,
            &id,
            "--reason",
            "No longer needed",
            "--output",
            "json",
        ])
        .await;
    assert_success(&canceled);
    assert_eq!(body(&canceled)["data"]["state"], "canceled");
    assert_eq!(body(&canceled)["data"]["state_reason"], "No longer needed");
    assert_removed_task_names(&server, &space, &id).await;
}

async fn assert_removed_task_names(server: &Server, space: &str, id: &str) {
    for args in [
        vec!["task", "close", space, id, "--output", "json"],
        vec![
            "task", "list", space, "--status", "working", "--output", "json",
        ],
    ] {
        assert_usage(&server.run(&args).await);
    }
}
