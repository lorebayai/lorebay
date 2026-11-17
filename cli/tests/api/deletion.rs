use serde_json::json;

use crate::helpers::{
    append_note, assert_cancelled, assert_deletion, assert_success, assert_task_absent,
    assert_task_error, body, create_task, notes_output, piped_yes, printed_note_time, read_task,
    task_context, task_id_from_show,
};

#[tokio::test]
async fn task_delete_removes_task() {
    let (server, space, agent) = task_context().await;
    let guard = create_task(&server, &space, &agent, "Keep task").await;
    for mode in ["human", "json"] {
        let task = create_task(&server, &space, &agent, "Remove task").await;
        append_note(&server, &space, &task, &agent, "Remove note with task").await;
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
async fn note_delete_removes_note_named_by_printed_time() {
    let (server, space, agent) = task_context().await;
    let task = create_task(&server, &space, &agent, "Remove individual notes").await;
    let id = task["task_id"].as_str().unwrap();
    let key = task["key"].as_str().unwrap();
    let mut notes = Vec::new();
    for content in [
        "Read time from human output",
        "Read time from JSON output",
        "Keep note",
    ] {
        let time = append_note(&server, &space, &task, &agent, content).await;
        notes.push(json!({"time": time, "content": content}));
    }
    for (index, mode) in ["human", "json"].into_iter().enumerate() {
        let printed = notes_output(&server, &space, key, mode).await;
        let time = printed_note_time(&printed, mode);
        assert_eq!(time, notes[index]["time"].to_string());
        let wrong_key = server
            .run(&[
                "note", "delete", &space, key, &time, "--yes", "--output", mode,
            ])
            .await;
        assert_task_error(&wrong_key, mode, "task.not_found");
        let deleted = server
            .run(&[
                "note", "delete", &space, id, &time, "--yes", "--output", mode,
            ])
            .await;
        assert_deletion(&deleted, mode, "Note deleted.");
        let remaining = notes_output(&server, &space, key, "json").await;
        assert_eq!(body(&remaining)["data"], json!(&notes[index + 1..]));
        let gone = server
            .run(&[
                "note", "delete", &space, id, &time, "--yes", "--output", mode,
            ])
            .await;
        assert_task_error(&gone, mode, "note.not_found");
    }
}

#[tokio::test]
async fn note_delete_cancels_when_input_is_not_terminal() {
    let (server, space, agent) = task_context().await;
    let task = create_task(&server, &space, &agent, "Keep note").await;
    let time = append_note(&server, &space, &task, &agent, "Keep this note")
        .await
        .to_string();
    let id = task["task_id"].as_str().unwrap();
    let key = task["key"].as_str().unwrap();
    let before = notes_output(&server, &space, key, "json").await;
    for mode in ["human", "json"] {
        let output = piped_yes(
            &server,
            &["note", "delete", &space, id, &time, "--output", mode],
        )
        .await;
        assert_cancelled(
            &output,
            mode,
            &format!("note {time} of task {id} in {space}"),
        );
        let after = notes_output(&server, &space, key, "json").await;
        assert_eq!(body(&after), body(&before));
    }
}
