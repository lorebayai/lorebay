use std::process::Output;

use reqwest::Response;
use serde_json::{Value, json};
use url::Url;

use super::{
    Server, assert_cancelled, assert_success, assert_task_error, body, piped_yes, read_task,
};

pub fn printed_artifact_url(output: &Output, mode: &str) -> Url {
    let text = if mode == "json" {
        let response = body(output);
        assert_eq!(response.as_object().unwrap().len(), 2);
        assert_eq!(response["meta"], json!({}));
        assert_eq!(response["data"].as_object().unwrap().len(), 2);
        response["data"]["url"].as_str().unwrap().to_owned()
    } else {
        let text = String::from_utf8(output.stdout.clone()).unwrap();
        assert_eq!(text.lines().count(), 1);
        text.strip_suffix('\n').unwrap().to_owned()
    };
    let url = Url::parse(&text).unwrap();
    let expires = url
        .query_pairs()
        .find(|(key, _)| key == "expires")
        .unwrap()
        .1
        .parse::<i64>()
        .unwrap();
    if mode == "json" {
        assert_eq!(body(output)["data"]["expires_at"], expires);
    }
    url
}

pub async fn download_url(server: &Server, space: &str, key: &str, mode: &str) -> Url {
    let output = server
        .run(&["artifact", "url", space, key, "--output", mode])
        .await;
    assert_success(&output);
    assert!(output.stderr.is_empty());
    let url = printed_artifact_url(&output, mode);
    assert_eq!(url.origin().ascii_serialization(), server.url);
    url
}

pub async fn fetch_artifact(server: &Server, url: &Url) -> Response {
    server
        .client
        .get(url.clone())
        .send()
        .await
        .map_err(|_| "artifact download failed")
        .unwrap()
}

pub async fn assert_artifact_delete_refusals(
    server: &Server,
    space: &str,
    task: &Value,
    mode: &str,
) {
    let key = task["artifacts"][0]["key"].as_str().unwrap();
    let task_key = task["key"].as_str().unwrap();
    let id = task["task_id"].as_str().unwrap();
    let cancelled = piped_yes(
        server,
        &["artifact", "delete", space, id, key, "--output", mode],
    )
    .await;
    assert_cancelled(
        &cancelled,
        mode,
        &format!("artifact {key} of task {id} in {space}"),
    );
    let refused = server
        .run(&[
            "artifact", "delete", space, task_key, key, "--yes", "--output", mode,
        ])
        .await;
    assert_task_error(&refused, mode, "task.not_found");
    assert_eq!(read_task(server, space, task_key).await, *task);
}
