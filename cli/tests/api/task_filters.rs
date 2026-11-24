use serde_json::json;

use crate::helpers::{
    FILTER_AGENT, FILTER_KEYWORD, assert_filter_errors, assert_filtered_pages, assert_success,
    body, filtered_tasks,
};

#[tokio::test]
async fn task_list_returns_matching_tasks_when_filtered() {
    let (mut server, space) = filtered_tasks().await;
    for mode in ["human", "json"] {
        assert_filtered_pages(
            &server,
            &space,
            &[
                "--person",
                &server.username,
                "--agent",
                FILTER_AGENT,
                "--state",
                "working",
                "--keyword",
                FILTER_KEYWORD,
            ],
            mode,
        )
        .await;
        assert_filtered_pages(
            &server,
            &space,
            &[
                "--own-open",
                "--agent",
                FILTER_AGENT,
                "--keyword",
                FILTER_KEYWORD,
            ],
            mode,
        )
        .await;
    }
    let ended = server
        .run(&[
            "task", "list", &space, "--state", "canceled", "--output", "json",
        ])
        .await;
    assert_success(&ended);
    assert_eq!(body(&ended)["data"].as_array().unwrap().len(), 1);
    assert_eq!(body(&ended)["data"][0]["key"], "TK-6");
    let empty = server
        .run(&[
            "task", "list", &space, "--person", "unknown", "--output", "json",
        ])
        .await;
    assert_success(&empty);
    assert_eq!(body(&empty), json!({"data": [], "meta": {}}));
    assert!(empty.stderr.is_empty());
    assert_filter_errors(&mut server, &space).await;
}
