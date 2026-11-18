use reqwest::StatusCode;
use serde_json::{Value, json};

use crate::helpers::{
    ARTIFACT_BYTES, assert_artifact_delete_refusals, assert_deletion, assert_success,
    assert_task_error, body, download_url, fetch_artifact, printed_artifact_url, task_context,
    task_with_artifacts,
};

#[tokio::test]
async fn artifact_url_prints_url_that_serves_uploaded_bytes() {
    let (server, space, agent) = task_context().await;
    let task =
        task_with_artifacts(&server, &space, &agent, "Download artifact", "Report ready").await;
    let key = task["artifacts"][0]["key"].as_str().unwrap();
    for mode in ["human", "json"] {
        let output = server
            .run(&["artifact", "url", &space, key, "--output", mode])
            .await;
        assert_success(&output);
        assert!(output.stderr.is_empty());
        let url = printed_artifact_url(&output, mode);
        assert_eq!(url.origin().ascii_serialization(), server.url);
        let response = server
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| "artifact download failed")
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.bytes().await.unwrap(), ARTIFACT_BYTES.as_bytes());
        for invalid in ["AR-3", "AR-999", "AR-1/../AR-1?query#fragment%"] {
            let refused = server
                .run(&["artifact", "url", &space, invalid, "--output", mode])
                .await;
            assert_task_error(&refused, mode, "artifact.not_found");
        }
    }
}

#[tokio::test]
async fn artifact_delete_removes_artifact_from_task() {
    let (server, space, agent) = task_context().await;
    for mode in ["human", "json"] {
        let task =
            task_with_artifacts(&server, &space, &agent, "Delete artifact", "Keep result").await;
        let id = task["task_id"].as_str().unwrap();
        let key = task["artifacts"][0]["key"].as_str().unwrap();
        let url = download_url(&server, &space, key, mode).await;
        let before = fetch_artifact(&server, &url).await;
        assert_eq!(before.status(), StatusCode::OK);
        assert_eq!(before.bytes().await.unwrap(), ARTIFACT_BYTES.as_bytes());
        assert_artifact_delete_refusals(&server, &space, &task, mode).await;
        let deleted = server
            .run(&[
                "artifact", "delete", &space, id, key, "--yes", "--output", mode,
            ])
            .await;
        assert_deletion(&deleted, mode, "Artifact deleted.");
        let task_key = task["key"].as_str().unwrap();
        let shown = server
            .run(&["task", "show", &space, task_key, "--output", "json"])
            .await;
        assert_success(&shown);
        let mut expected = task.clone();
        expected["artifacts"] = json!(&task["artifacts"].as_array().unwrap()[1..]);
        assert_eq!(body(&shown), json!({"data": expected, "meta": {}}));
        let after = fetch_artifact(&server, &url).await;
        assert_eq!(after.status(), StatusCode::NOT_FOUND);
        let problem: Value = after.json().await.unwrap();
        assert_eq!(problem["code"], "file.not_found");
        let missing = server
            .run(&["artifact", "url", &space, key, "--output", mode])
            .await;
        assert_task_error(&missing, mode, "artifact.not_found");
        let kept_key = task["artifacts"][1]["key"].as_str().unwrap();
        let kept_url = download_url(&server, &space, kept_key, mode).await;
        let kept = fetch_artifact(&server, &kept_url).await;
        assert_eq!(kept.status(), StatusCode::OK);
        assert_eq!(kept.bytes().await.unwrap(), ARTIFACT_BYTES.as_bytes());
    }
}
