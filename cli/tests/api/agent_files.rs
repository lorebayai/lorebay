use reqwest::StatusCode;
use serde_json::{Value, json};

use crate::helpers::{
    AGENT_FILE_BYTES, assert_agent_file_delete_refusals, assert_deletion, assert_success,
    assert_task_error, body, download_url, fetch_agent_file, printed_agent_file_url, task_context,
    task_with_agent_files,
};

#[tokio::test]
async fn agent_file_url_prints_url_that_serves_uploaded_bytes() {
    let (server, space, agent) = task_context().await;
    let task =
        task_with_agent_files(&server, &space, &agent, "Download file", "Report ready").await;
    let key = task["files"][0]["key"].as_str().unwrap();
    for mode in ["human", "json"] {
        let output = server
            .run(&["file", "url", &space, key, "--output", mode])
            .await;
        assert_success(&output);
        assert!(output.stderr.is_empty());
        let url = printed_agent_file_url(&output, mode);
        assert_eq!(url.origin().ascii_serialization(), server.url);
        let response = server
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| "file download failed")
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.bytes().await.unwrap(), AGENT_FILE_BYTES.as_bytes());
        for invalid in ["FL-3", "FL-999", "FL-1/../FL-1?query#fragment%"] {
            let refused = server
                .run(&["file", "url", &space, invalid, "--output", mode])
                .await;
            assert_task_error(&refused, mode, "agent_file.not_found");
        }
    }
}

#[tokio::test]
async fn agent_file_delete_removes_agent_file_from_task() {
    let (server, space, agent) = task_context().await;
    for mode in ["human", "json"] {
        let task =
            task_with_agent_files(&server, &space, &agent, "Delete file", "Keep result").await;
        let id = task["task_id"].as_str().unwrap();
        let key = task["files"][0]["key"].as_str().unwrap();
        let url = download_url(&server, &space, key, mode).await;
        let before = fetch_agent_file(&server, &url).await;
        assert_eq!(before.status(), StatusCode::OK);
        assert_eq!(before.bytes().await.unwrap(), AGENT_FILE_BYTES.as_bytes());
        assert_agent_file_delete_refusals(&server, &space, &task, mode).await;
        let deleted = server
            .run(&["file", "delete", &space, id, key, "--yes", "--output", mode])
            .await;
        assert_deletion(&deleted, mode, "File deleted.");
        let task_key = task["key"].as_str().unwrap();
        let shown = server
            .run(&["task", "show", &space, task_key, "--output", "json"])
            .await;
        assert_success(&shown);
        let mut expected = task.clone();
        expected["files"] = json!(&task["files"].as_array().unwrap()[1..]);
        assert_eq!(body(&shown), json!({"data": expected, "meta": {}}));
        let after = fetch_agent_file(&server, &url).await;
        assert_eq!(after.status(), StatusCode::NOT_FOUND);
        let problem: Value = after.json().await.unwrap();
        assert_eq!(problem["code"], "file.not_found");
        let missing = server
            .run(&["file", "url", &space, key, "--output", mode])
            .await;
        assert_task_error(&missing, mode, "agent_file.not_found");
        let kept_key = task["files"][1]["key"].as_str().unwrap();
        let kept_url = download_url(&server, &space, kept_key, mode).await;
        let kept = fetch_agent_file(&server, &kept_url).await;
        assert_eq!(kept.status(), StatusCode::OK);
        assert_eq!(kept.bytes().await.unwrap(), AGENT_FILE_BYTES.as_bytes());
    }
}
