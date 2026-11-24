use reqwest::Method;
use serde_json::{Value, json};

use crate::helpers::{
    Server, assert_success, assert_task_error, body, create_task, read_task, take_over_task,
    task_context, task_pages,
};

#[tokio::test]
async fn task_results_prints_version_history_with_each_writer() {
    let (server, space, agent) = task_context().await;
    let task = create_task(&server, &space, &agent, "Results").await;
    let empty = server
        .run(&["task", "results", &space, "TK-1", "--output", "json"])
        .await;
    assert_success(&empty);
    assert_eq!(body(&empty), json!({"data":[],"meta":{}}));
    let peer = server.agent("Continuing agent").await;
    let mut expected = Vec::new();
    for (index, (writer, result)) in [
        (&agent, "First\n- detail\tcode\u{1b}[0m"),
        (&peer, "Second"),
        (&peer, ""),
    ]
    .into_iter()
    .enumerate()
    {
        if index == 1 {
            take_over_task(&server, &task, &peer).await;
        }
        let changed = server
            .json(
                Method::PATCH,
                &format!(
                    "/api/v1/spaces/{space}/tasks/{}",
                    task["task_id"].as_str().unwrap()
                ),
                json!({"result":result,"result_version":(index > 0).then_some(index)}),
                writer["access_token"].as_str(),
            )
            .await;
        expected.push(json!({
            "version":index + 1,"result":result,"person":server.username,
            "agent_name":if index == 0 { "Task agent" } else { "Continuing agent" },
            "created_at":changed["data"]["last_activity_at"]
        }));
    }
    expected.reverse();
    assert_eq!(
        read_task(&server, &space, "TK-1").await["result_version"],
        3
    );
    for mode in ["human", "json"] {
        let (first, last, _) =
            task_pages(&server, &["task", "results", &space, "TK-1"], mode).await;
        if mode == "json" {
            assert_eq!(body(&first)["data"], json!(expected[..2]));
            assert_eq!(body(&last), json!({"data":[expected[2]],"meta":{}}));
        } else {
            assert_human_history(&server, &first.stdout, &last.stdout, &expected);
        }
        let missing = server
            .run(&["task", "results", &space, "TK-999", "--output", mode])
            .await;
        assert_task_error(&missing, mode, "task.not_found");
    }
}

fn assert_human_history(server: &Server, first: &[u8], last: &[u8], expected: &[Value]) {
    let first = String::from_utf8_lossy(first);
    assert!(first.starts_with("Version\tPerson\tAgent name\tCreated at\tResult\n"));
    for (index, row) in expected[..2].iter().enumerate() {
        let number = 3 - index;
        let time = &row["created_at"];
        let result = if index == 0 { "" } else { "Second" };
        assert!(first.contains(&format!(
            "{number}\t{}\tContinuing agent\t{time}\t{result}\n",
            server.username
        )));
    }
    let time = &expected[2]["created_at"];
    assert!(String::from_utf8_lossy(last).contains(&format!(
        "1\t{}\tTask agent\t{time}\tFirst\n- detail\tcode\\u{{1b}}[0m\n",
        server.username
    )));
}
