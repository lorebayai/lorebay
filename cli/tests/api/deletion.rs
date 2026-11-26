use serde_json::json;

use crate::helpers::{
    append_checkpoint, assert_cancelled, assert_deletion, assert_success, assert_task_absent,
    assert_task_error, body, checkpoints_output, create_task, expected_time, piped_yes,
    printed_checkpoint_time, read_task, task_context, task_id_from_show,
};

#[tokio::test]
async fn task_delete_removes_task() {
    let (server, space, agent) = task_context().await;
    let guard = create_task(&server, &space, &agent, "Keep task").await;
    for mode in ["human", "json"] {
        let task = create_task(&server, &space, &agent, "Remove task").await;
        append_checkpoint(
            &server,
            &space,
            &task,
            &agent,
            "Remove checkpoint with task",
        )
        .await;
        let key = task["key"].as_str().unwrap();
        let id = task_id_from_show(&server, &space, key, mode).await;
        let expected = read_task(&server, &space, key).await;
        let cancelled = server
            .run(&["task", "delete", &space, &id, "--output", mode])
            .await;
        assert_cancelled(&cancelled, mode, &format!("task {id} in {space}"));
        assert_eq!(read_task(&server, &space, key).await, expected);
        let wrong_key = server
            .run(&["task", "delete", &space, key, "--yes", "--output", mode])
            .await;
        assert_task_error(&wrong_key, mode, "task.not_found");
        let deleted = server
            .run(&["task", "delete", &space, &id, "--yes", "--output", mode])
            .await;
        assert_deletion(&deleted, mode, "Task deleted.");
        assert_task_absent(&server, &space, key).await;
        let list = server
            .run(&["task", "list", &space, "--output", "json"])
            .await;
        assert_success(&list);
        assert_eq!(body(&list)["data"].as_array().unwrap().len(), 1);
        assert_eq!(body(&list)["data"][0]["key"], guard["key"]);
    }
}

#[tokio::test]
async fn checkpoint_delete_removes_checkpoint_named_by_printed_time() {
    let (server, space, agent) = task_context().await;
    let task = create_task(&server, &space, &agent, "Remove individual checkpoints").await;
    let id = task["task_id"].as_str().unwrap();
    let key = task["key"].as_str().unwrap();
    let mut checkpoints = Vec::new();
    for content in [
        "Read time from human output",
        "Read time from JSON output",
        "Keep checkpoint",
    ] {
        let time = append_checkpoint(&server, &space, &task, &agent, content).await;
        checkpoints.push(json!({"time": time, "agent_name":"Task agent", "content": content}));
    }
    for (index, mode) in ["human", "json"].into_iter().enumerate() {
        let printed = checkpoints_output(&server, &space, key, mode).await;
        let time = printed_checkpoint_time(&printed, mode);
        assert_eq!(time, expected_time(&checkpoints[index]["time"], mode));
        let wrong_key = server
            .run(&[
                "checkpoint",
                "delete",
                &space,
                key,
                &time,
                "--yes",
                "--output",
                mode,
            ])
            .await;
        assert_task_error(&wrong_key, mode, "task.not_found");
        let deleted = server
            .run(&[
                "checkpoint",
                "delete",
                &space,
                id,
                &time,
                "--yes",
                "--output",
                mode,
            ])
            .await;
        assert_deletion(&deleted, mode, "Checkpoint deleted.");
        let remaining = checkpoints_output(&server, &space, key, "json").await;
        assert_eq!(body(&remaining)["data"], json!(&checkpoints[index + 1..]));
        let gone = server
            .run(&[
                "checkpoint",
                "delete",
                &space,
                id,
                &time,
                "--yes",
                "--output",
                mode,
            ])
            .await;
        assert_task_error(&gone, mode, "checkpoint.not_found");
    }
}

#[tokio::test]
async fn checkpoint_delete_cancels_when_input_is_not_terminal() {
    let (server, space, agent) = task_context().await;
    let task = create_task(&server, &space, &agent, "Keep checkpoint").await;
    let time = append_checkpoint(&server, &space, &task, &agent, "Keep this checkpoint")
        .await
        .to_string();
    let id = task["task_id"].as_str().unwrap();
    let key = task["key"].as_str().unwrap();
    let before = checkpoints_output(&server, &space, key, "json").await;
    for mode in ["human", "json"] {
        let output = piped_yes(
            &server,
            &["checkpoint", "delete", &space, id, &time, "--output", mode],
        )
        .await;
        assert_cancelled(
            &output,
            mode,
            &format!("checkpoint {time} of task {id} in {space}"),
        );
        let after = checkpoints_output(&server, &space, key, "json").await;
        assert_eq!(body(&after), body(&before));
    }
}
