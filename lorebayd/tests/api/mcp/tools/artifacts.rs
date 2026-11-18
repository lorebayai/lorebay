use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};

use crate::artifacts::helpers::{complete, registration};
use crate::helpers::{Server, authorize_for, create_space, list_response, login};
use crate::tasks::helpers::{self as tasks, SPACE};

use super::{NEWEST_VERSION, call, call_error, call_version, initialize};

#[tokio::test]
async fn call_create_artifact_then_upload_then_complete_lists_artifact() {
    let (server, person, agent) = tasks::setup().await;
    let peer = authorize_for(&server, &person, "Completing agent").await;
    for version in [NEWEST_VERSION, "2025-03-26", "2025-06-18", "2025-11-25"] {
        if version != NEWEST_VERSION {
            initialize(&server, &peer, version).await;
        }
        let task = call_version(
            &server,
            &agent,
            "create_task",
            json!({"space":SPACE, "title":version}),
            version,
        )
        .await;
        let registered = call_version(
            &server,
            &agent,
            "create_artifact",
            json!({"task_id":task["task_id"], "file_name":"report.txt", "summary":"The report"}),
            version,
        )
        .await;
        assert_eq!(registered["task_title"], version);
        assert!(registered["key"].as_str().unwrap().starts_with("AR-"));
        assert!(registered["expires_at"].as_i64().unwrap() > 0);
        let before = read_task(&server, &agent, &task).await;
        assert_eq!(before["artifacts"], json!([]));
        assert_eq!(
            upload(&server, &registered, "The file").await.status(),
            StatusCode::CREATED
        );
        assert_eq!(read_task(&server, &agent, &task).await, before);
        let completed = call_version(
            &server,
            &peer,
            "complete_artifact",
            json!({"task_id":task["task_id"], "key":registered["key"]}),
            version,
        )
        .await;
        assert_eq!(completed["key"], registered["key"]);
        assert_eq!(completed["task_title"], version);
        assert_eq!(completed["file_name"], "report.txt");
        assert_eq!(completed["summary"], "The report");
        assert_eq!(completed["size"], 8);
        assert!(
            completed["completed_at"].as_i64().unwrap()
                >= completed["created_at"].as_i64().unwrap()
        );
        assert_completed(&server, &person, &agent, &task, &completed).await;
    }
    server.stop().await;
}

#[tokio::test]
async fn call_create_artifact_fails_with_error_result_when_file_name_is_empty() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Valid title").await;
    for body in [
        json!({"file_name":"", "summary":"Valid"}),
        json!({"file_name":"   ", "summary":"Valid"}),
        json!({"file_name":"x".repeat(256), "summary":"Valid"}),
        json!({"file_name":"\u{754c}".repeat(86), "summary":"Valid"}),
        json!({"file_name":"report.txt", "summary":""}),
        json!({"file_name":"report.txt", "summary":"x".repeat(501)}),
        json!({"file_name":"report.txt", "summary":"Valid", "unknown":"private-sentinel"}),
        json!({"file_name":"report.txt", "summary":"Valid", "space":SPACE}),
        json!({"file_name":false, "summary":"Valid"}),
        json!({"file_name":null, "summary":"Valid"}),
        json!({"file_name":"report.txt", "summary":false}),
        json!({"file_name":"report.txt"}),
        json!({"summary":"Valid"}),
    ] {
        let mut arguments = body.clone();
        arguments["task_id"] = task["task_id"].clone();
        let error = call_error(&server, &agent, "create_artifact", arguments).await;
        let response = registration(
            &server,
            &agent,
            SPACE,
            task["task_id"].as_str().unwrap(),
            body,
        )
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_problem_matches(error, response).await;
    }
    assert_eq!(
        tasks::read(&server, &person, "TK-1").await["artifacts"],
        json!([])
    );
    let registered = call(&server, &agent, "create_artifact", json!({
        "task_id":task["task_id"], "file_name":"\u{754c}".repeat(85), "summary":"\u{754c}".repeat(500)
    })).await;
    assert_eq!(registered["key"], "AR-1");
    assert_eq!(
        upload(&server, &registered, "").await.status(),
        StatusCode::CREATED
    );
    let completed = call(
        &server,
        &agent,
        "complete_artifact",
        json!({
            "task_id":task["task_id"], "key":registered["key"]
        }),
    )
    .await;
    assert_eq!(completed["file_name"], "\u{754c}".repeat(85));
    assert_eq!(completed["summary"], "\u{754c}".repeat(500));
    assert_eq!(completed["size"], 0);
    assert_completed(&server, &person, &agent, &task, &completed).await;
    server.stop().await;
}

#[tokio::test]
async fn call_complete_artifact_succeeds_when_upload_returned_precondition_failed() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Retry upload").await;
    let registered = register(&server, &agent, &task).await;
    assert_eq!(
        upload(&server, &registered, "Original").await.status(),
        StatusCode::CREATED
    );
    let retry = upload(&server, &registered, "Overwrite refused").await;
    assert_eq!(retry.status(), StatusCode::PRECONDITION_FAILED);
    assert_eq!(
        retry.json::<Value>().await.unwrap()["code"],
        "file.already_exists"
    );
    let arguments = json!({"task_id":task["task_id"], "key":registered["key"]});
    let completed = call(&server, &agent, "complete_artifact", arguments.clone()).await;
    assert_eq!(completed["task_title"], "Retry upload");
    assert_eq!(completed["size"], 8);
    assert_completed(&server, &person, &agent, &task, &completed).await;
    let before = read_task(&server, &agent, &task).await;
    assert_eq!(
        call(&server, &agent, "complete_artifact", arguments).await,
        completed
    );
    assert_eq!(read_task(&server, &agent, &task).await, before);
    assert_eq!(
        complete(&server, &agent, &task, "AR-1")
            .await
            .json::<Value>()
            .await
            .unwrap()["data"],
        completed
    );
    server.stop().await;
}

#[tokio::test]
async fn call_complete_artifact_fails_when_file_was_not_uploaded() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Pending upload").await;
    let registered = register(&server, &agent, &task).await;
    for key in ["AR-1", "AR-999", "AR-01", "TK-1"] {
        let error = call_error(
            &server,
            &agent,
            "complete_artifact",
            json!({
                "task_id":task["task_id"], "key":key
            }),
        )
        .await;
        assert_eq!(
            error["code"],
            if key == "AR-1" {
                "artifact.not_uploaded"
            } else {
                "artifact.not_found"
            }
        );
        assert_problem_matches(error, complete(&server, &agent, &task, key).await).await;
    }
    let other = tasks::create(&server, &agent, SPACE, "Other task").await;
    let other_artifact = register(&server, &agent, &other).await;
    assert_eq!(
        call_error(
            &server,
            &agent,
            "complete_artifact",
            json!({
                "task_id":task["task_id"], "key":other_artifact["key"]
            })
        )
        .await["code"],
        "artifact.not_found"
    );
    assert_invalid_completion_arguments(&server, &agent, &task).await;
    assert_eq!(
        tasks::read(&server, &person, "TK-1").await["artifacts"],
        json!([])
    );
    assert_eq!(
        upload(&server, &registered, "Recovered").await.status(),
        StatusCode::CREATED
    );
    let completed = call(
        &server,
        &agent,
        "complete_artifact",
        json!({
            "task_id":task["task_id"], "key":registered["key"]
        }),
    )
    .await;
    assert_eq!(completed["size"], 9);
    assert_completed(&server, &person, &agent, &task, &completed).await;
    server.stop().await;
}

#[tokio::test]
async fn call_create_artifact_fails_when_task_is_outside_writable_party() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Alice task").await;
    register(&server, &agent, &task).await;
    let bob = login(&server, "bob").await;
    let bob_agent = authorize_for(&server, &bob, "Bob agent").await;
    for id in ["unknown", "TK-1", task["task_id"].as_str().unwrap()] {
        assert_refusals(&server, &bob_agent, id, "task.not_found").await;
    }
    tasks::participant(&server, &person, Method::POST, "contributor").await;
    assert_refusals(
        &server,
        &bob_agent,
        task["task_id"].as_str().unwrap(),
        "space.permission_required",
    )
    .await;
    let own = tasks::create(&server, &bob_agent, SPACE, "Bob task").await;
    tasks::participant(&server, &person, Method::PATCH, "viewer").await;
    assert_refusals(
        &server,
        &bob_agent,
        own["task_id"].as_str().unwrap(),
        "space.permission_required",
    )
    .await;
    tasks::participant(&server, &person, Method::DELETE, "").await;
    assert_refusals(
        &server,
        &bob_agent,
        own["task_id"].as_str().unwrap(),
        "task.not_found",
    )
    .await;
    assert_eq!(
        tasks::read(&server, &person, "TK-1").await["artifacts"],
        json!([])
    );
    assert_eq!(
        tasks::read(&server, &person, "TK-2").await["artifacts"],
        json!([])
    );
    assert_eq!(register(&server, &agent, &task).await["key"], "AR-2");
    server.stop().await;
}

#[tokio::test]
async fn call_create_artifact_fails_with_reopen_hint_when_task_is_ended() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Before ending").await;
    let registered = register(&server, &agent, &task).await;
    let ended = call(
        &server,
        &agent,
        "update_task",
        json!({
            "task_id":task["task_id"], "title":"Paused upload", "status":"ended"
        }),
    )
    .await;
    let error = call_error(
        &server,
        &agent,
        "create_artifact",
        json!({
            "task_id":task["task_id"], "file_name":"rejected.txt", "summary":"Rejected"
        }),
    )
    .await;
    assert_eq!(error["code"], "task.ended");
    assert_eq!(
        error["message"],
        "Set the task back to in progress before registering an artifact"
    );
    assert_eq!(
        upload(&server, &registered, "Pending").await.status(),
        StatusCode::CREATED
    );
    let completed = call(
        &server,
        &agent,
        "complete_artifact",
        json!({
            "task_id":task["task_id"], "key":registered["key"]
        }),
    )
    .await;
    assert_eq!(completed["task_title"], "Paused upload");
    assert_completed(&server, &person, &agent, &task, &completed).await;
    let after = read_task(&server, &agent, &task).await;
    assert_eq!(after["status"], "ended");
    assert_eq!(after["ended_at"], ended["ended_at"]);
    call(
        &server,
        &agent,
        "update_task",
        json!({
            "task_id":task["task_id"], "status":"in progress"
        }),
    )
    .await;
    assert_eq!(register(&server, &agent, &task).await["key"], "AR-2");
    server.stop().await;
}

#[tokio::test]
async fn call_get_artifact_url_returns_url_that_serves_file() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Shared artifact").await;
    let registered = register(&server, &agent, &task).await;
    assert_eq!(
        upload(&server, &registered, "Download bytes\0")
            .await
            .status(),
        StatusCode::CREATED
    );
    call(
        &server,
        &agent,
        "complete_artifact",
        json!({
            "task_id":task["task_id"], "key":registered["key"]
        }),
    )
    .await;
    let bob = login(&server, "bob").await;
    tasks::participant(&server, &person, Method::POST, "viewer").await;
    let reader = authorize_for(&server, &bob, "Artifact reader").await;
    let before = tasks::read(&server, &person, "TK-1").await;
    for version in [NEWEST_VERSION, "2025-03-26", "2025-06-18", "2025-11-25"] {
        if version != NEWEST_VERSION {
            initialize(&server, &reader, version).await;
        }
        let download = call_version(
            &server,
            &reader,
            "get_artifact_url",
            json!({
                "space":SPACE, "key":registered["key"]
            }),
            version,
        )
        .await;
        assert!(download["expires_at"].as_i64().unwrap() > 0);
        assert!(
            download["url"]
                .as_str()
                .unwrap()
                .starts_with(&format!("{}/files/", server.public_url))
        );
        let response = server
            .client
            .get(download["url"].as_str().unwrap())
            .send()
            .await
            .map_err(|_| "download request failed")
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.bytes().await.unwrap(), "Download bytes\0");
    }
    assert_eq!(read_task(&server, &reader, &task).await, before);
    assert_eq!(tasks::read(&server, &person, "TK-1").await, before);
    server.stop().await;
}

#[tokio::test]
async fn call_get_artifact_url_fails_when_artifact_is_not_readable() {
    let (server, person, agent) = tasks::setup().await;
    let task = tasks::create(&server, &agent, SPACE, "Pending artifact").await;
    let registered = register(&server, &agent, &task).await;
    create_space(&server, &person, "Other", "Other space").await;
    for (space, key) in [
        (SPACE, "AR-1"),
        (SPACE, "AR-999"),
        (SPACE, "TK-1"),
        ("SPACE-2", "AR-1"),
    ] {
        let error = call_error(
            &server,
            &agent,
            "get_artifact_url",
            json!({"space":space, "key":key}),
        )
        .await;
        assert_eq!(error["code"], "artifact.not_found");
        let response = list_response(
            &format!("/api/v1/spaces/{space}/artifacts/{key}/url"),
            &server,
            &agent,
            &[],
        )
        .await;
        assert_problem_matches(error, response).await;
    }
    let bob = login(&server, "bob").await;
    let outsider = authorize_for(&server, &bob, "Outsider").await;
    assert_eq!(
        call_error(
            &server,
            &outsider,
            "get_artifact_url",
            json!({
                "space":SPACE, "key":registered["key"]
            })
        )
        .await["code"],
        "space.not_found"
    );
    for arguments in [
        json!({"key":"AR-1"}),
        json!({"space":SPACE}),
        json!({"space":SPACE, "key":null}),
        json!({"space":false, "key":"AR-1"}),
        json!({"space":SPACE, "key":"AR-1", "task_id":task["task_id"]}),
    ] {
        let error = call_error(&server, &agent, "get_artifact_url", arguments).await;
        assert_eq!(error["code"], "request.invalid_body");
        assert!(!error["errors"].as_array().unwrap().is_empty());
    }
    assert_eq!(
        read_task(&server, &agent, &task).await["artifacts"],
        json!([])
    );
    server.stop().await;
}

async fn register(server: &Server, agent: &Value, task: &Value) -> Value {
    call(
        server,
        agent,
        "create_artifact",
        json!({
            "task_id":task["task_id"], "file_name":"report.txt", "summary":"The report"
        }),
    )
    .await
}

async fn upload(server: &Server, registered: &Value, bytes: &'static str) -> Response {
    assert_eq!(registered["method"], "PUT");
    assert_eq!(registered["headers"], json!({"If-None-Match":"*"}));
    let method = Method::from_bytes(registered["method"].as_str().unwrap().as_bytes()).unwrap();
    let mut request = server
        .client
        .request(method, registered["upload_url"].as_str().unwrap())
        .body(bytes);
    for (name, value) in registered["headers"].as_object().unwrap() {
        request = request.header(name, value.as_str().unwrap());
    }
    request
        .send()
        .await
        .map_err(|_| "upload request failed")
        .unwrap()
}

async fn read_task(server: &Server, agent: &Value, task: &Value) -> Value {
    call(
        server,
        agent,
        "get_task",
        json!({"space":SPACE, "key":task["key"]}),
    )
    .await
}

async fn assert_completed(
    server: &Server,
    person: &Value,
    agent: &Value,
    task: &Value,
    completed: &Value,
) {
    let read = read_task(server, agent, task).await;
    assert_eq!(
        read,
        tasks::read(server, person, task["key"].as_str().unwrap()).await
    );
    let mut artifact = completed.clone();
    artifact.as_object_mut().unwrap().remove("task_title");
    assert_eq!(read["artifacts"], json!([artifact]));
    assert_eq!(read["last_activity_at"], completed["completed_at"]);
}

async fn assert_problem_matches(error: Value, response: Response) {
    let problem = response.json::<Value>().await.unwrap();
    assert_eq!(error["code"], problem["code"]);
    assert_eq!(error["message"], problem["title"]);
    assert_eq!(error["errors"], problem["errors"]);
    assert!(
        !error["errors"].as_array().unwrap().is_empty() || error["code"] != "request.invalid_body"
    );
    assert!(!error.to_string().contains("private-sentinel"));
}

async fn assert_refusals(server: &Server, agent: &Value, task_id: &str, code: &str) {
    for (tool, arguments) in [
        (
            "create_artifact",
            json!({"task_id":task_id, "file_name":"a", "summary":"b"}),
        ),
        (
            "complete_artifact",
            json!({"task_id":task_id, "key":"AR-1"}),
        ),
    ] {
        assert_eq!(
            call_error(server, agent, tool, arguments).await["code"],
            code
        );
    }
}

async fn assert_invalid_completion_arguments(server: &Server, agent: &Value, task: &Value) {
    for arguments in [
        json!({"task_id":task["task_id"]}),
        json!({"task_id":task["task_id"], "key":false}),
        json!({"task_id":task["task_id"], "key":"AR-1", "unknown":"private-sentinel"}),
    ] {
        let error = call_error(server, agent, "complete_artifact", arguments).await;
        assert_eq!(error["code"], "request.invalid_body");
        assert!(!error.to_string().contains("private-sentinel"));
    }
}
