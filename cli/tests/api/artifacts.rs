use reqwest::StatusCode;

use crate::helpers::{
    ARTIFACT_BYTES, assert_success, assert_task_error, printed_artifact_url, task_context,
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
