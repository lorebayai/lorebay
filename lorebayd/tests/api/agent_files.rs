#[cfg(unix)]
use std::ffi::OsStr;
#[cfg(unix)]
use std::fs::{self, File, FileTimes};
#[cfg(unix)]
use std::os::unix::{ffi::OsStrExt, fs::symlink};
use std::time::Duration;
#[cfg(unix)]
use std::time::UNIX_EPOCH;

use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::yield_now;
use tokio::time::{sleep, timeout};
use url::Url;

use crate::helpers::{
    Server, TEST_PASSWORD, TEST_TIMEOUT, assert_problem, authorize_for, create_space, login,
    request,
};
use crate::tasks::helpers::{SPACE, append, create, read, setup, space_member};

use helpers::*;

pub(crate) mod helpers;

mod deletion;
mod download;

#[tokio::test]
async fn register_agent_file_returns_key_upload_url_and_headers() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "File title").await;
    let first = register(&server, &agent, &task).await;
    assert_eq!(first["key"], "FL-1");
    assert_eq!(first["task_title"], "File title");
    assert_eq!(first["method"], "PUT");
    assert_eq!(first["headers"], json!({"If-None-Match":"*"}));
    let url = Url::parse(first["upload_url"].as_str().unwrap()).unwrap();
    assert_eq!(url.origin().ascii_serialization(), server.public_url);
    assert!(url.path().starts_with("/files/"));
    assert!(!url.path().contains("report.txt"));
    let (second, third) = tokio::join!(
        register(&server, &agent, &task),
        register(&server, &agent, &task)
    );
    let mut keys = [
        second["key"].as_str().unwrap(),
        third["key"].as_str().unwrap(),
    ];
    keys.sort_unstable();
    assert_eq!(keys, ["FL-2", "FL-3"]);
    assert!(second["upload_url"] != third["upload_url"]);
    create_space(&server, &person, "Other", "Other work").await;
    let other = create(&server, &agent, "SPACE-2", "Other task").await;
    let response = registration(
        &server,
        &agent,
        "SPACE-2",
        other["task_id"].as_str().unwrap(),
        json!({"file_name":"report.txt", "summary":"Other"}),
    )
    .await;
    assert_eq!(
        response.json::<Value>().await.unwrap()["data"]["key"],
        "FL-1"
    );
    upload_ok(&server, &first, "Persisted file").await;
    let directory = server.stop().await;
    let server = Server::restart_with_users(
        directory,
        &[("alice", TEST_PASSWORD), ("bob", TEST_PASSWORD)],
    )
    .await;
    assert_eq!(
        complete(&server, &agent, &task, "FL-1").await.status(),
        StatusCode::OK
    );
    assert_eq!(register(&server, &agent, &task).await["key"], "FL-4");
    server.stop().await;
}

#[tokio::test]
async fn complete_agent_file_lists_agent_file_on_task_with_size() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "File title").await;
    let registered = register(&server, &agent, &task).await;
    let before = read(&server, &person, "TK-1").await;
    assert_eq!(before["files"], json!([]));
    upload_ok(&server, &registered, "The file").await;
    assert_eq!(read(&server, &person, "TK-1").await["files"], json!([]));
    let other = authorize_for(&server, &person, "Completing agent").await;
    let (first, concurrent) = tokio::join!(
        complete(&server, &other, &task, "FL-1"),
        complete(&server, &agent, &task, "FL-1")
    );
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(concurrent.status(), StatusCode::OK);
    let completed = first.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(concurrent.json::<Value>().await.unwrap()["data"], completed);
    assert_eq!(completed["size"], 8);
    assert_eq!(completed["task_title"], "File title");
    assert!(
        completed["completed_at"].as_i64().unwrap() >= completed["created_at"].as_i64().unwrap()
    );
    let after = read(&server, &person, "TK-1").await;
    assert!(after["last_activity_at"].as_i64() >= before["last_activity_at"].as_i64());
    assert_eq!(after["last_activity_at"], completed["completed_at"]);
    assert_eq!(after["files"][0]["file_name"], "report.txt");
    assert_eq!(after["files"][0]["summary"], "The report");
    append(&server, &agent, &task, "Later activity").await;
    append(&server, &agent, &task, "Still later").await;
    let after = read(&server, &person, "TK-1").await;
    assert!(after["last_activity_at"].as_i64() > completed["completed_at"].as_i64());
    assert_eq!(
        complete(&server, &agent, &task, "FL-1")
            .await
            .json::<Value>()
            .await
            .unwrap()["data"],
        completed
    );
    assert_eq!(read(&server, &person, "TK-1").await, after);
    let bob = login(&server, "bob").await;
    space_member(&server, &person, Method::POST, "viewer").await;
    assert_eq!(read(&server, &bob, "TK-1").await, after);
    let bob_agent = authorize_for(&server, &bob, "Reader").await;
    assert_eq!(read(&server, &bob_agent, "TK-1").await, after);
    server.stop().await;
}

#[tokio::test]
async fn upload_file_fails_with_forbidden_when_headers_are_missing() {
    let server = Server::start().await;
    let (_, agent, task) = prepare(&server).await;
    let registered = register(&server, &agent, &task).await;
    let url = registered["upload_url"].as_str().unwrap();
    for header in [None, Some("anything")] {
        let mut request = server.client.put(url).body("bytes");
        if let Some(header) = header {
            request = request.header("If-None-Match", header);
        }
        assert_problem(
            request.send().await.unwrap(),
            StatusCode::FORBIDDEN,
            "file.headers_required",
        )
        .await;
    }
    assert_problem(
        server
            .client
            .put(url)
            .header("If-None-Match", "*")
            .header("Content-Encoding", "identity")
            .body("bytes")
            .send()
            .await
            .map_err(|_| "upload request failed")
            .unwrap(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "file.content_encoding",
    )
    .await;
    upload_ok(&server, &registered, "bytes").await;
    server.stop().await;
}

#[tokio::test]
async fn upload_file_fails_with_precondition_failed_when_file_already_uploaded() {
    let server = Server::start().await;
    let (_, agent, task) = prepare(&server).await;
    let registered = register(&server, &agent, &task).await;
    let (first, second) = tokio::join!(
        upload(&server, &registered, "first"),
        upload(&server, &registered, "second")
    );
    let (success, rejected, size) = if first.status() == StatusCode::CREATED {
        (first, second, 5)
    } else {
        (second, first, 6)
    };
    assert_eq!(success.status(), StatusCode::CREATED);
    assert_problem(
        rejected,
        StatusCode::PRECONDITION_FAILED,
        "file.already_exists",
    )
    .await;
    assert_problem(
        upload(&server, &registered, "overwrite").await,
        StatusCode::PRECONDITION_FAILED,
        "file.already_exists",
    )
    .await;
    assert_eq!(
        complete(&server, &agent, &task, "FL-1")
            .await
            .json::<Value>()
            .await
            .unwrap()["data"]["size"],
        size
    );
    server.stop().await;
}

#[tokio::test]
async fn upload_file_succeeds_on_retry_when_first_attempt_was_interrupted() {
    let server = Server::configured(|config| config.upload_concurrency_limit = 1).await;
    let (_, agent, task) = prepare(&server).await;
    let registered = register(&server, &agent, &task).await;
    let mut stream = held_upload(&server, &registered).await;
    stream.write_all(b"a").await.unwrap();
    drop(stream);
    timeout(TEST_TIMEOUT, async {
        loop {
            let response = upload(&server, &registered, "retry").await;
            if response.status() == StatusCode::SERVICE_UNAVAILABLE {
                yield_now().await;
                continue;
            }
            assert_eq!(response.status(), StatusCode::CREATED);
            break;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        complete(&server, &agent, &task, "FL-1")
            .await
            .json::<Value>()
            .await
            .unwrap()["data"]["size"],
        5
    );
    server.stop().await;
}

#[tokio::test]
async fn upload_file_fails_when_file_exceeds_size_limit() {
    let server = Server::configured(|config| config.upload_size_limit_bytes = 4).await;
    let (_, agent, task) = prepare(&server).await;
    let registered = register(&server, &agent, &task).await;
    let url = registered["upload_url"].as_str().unwrap();
    let mut declared = raw_upload(&server, url, "Content-Length: 5\r\n").await;
    assert!(
        read_headers(&mut declared)
            .await
            .starts_with("HTTP/1.1 413")
    );
    drop(declared);
    assert_problem(
        upload(&server, &registered, "large").await,
        StatusCode::PAYLOAD_TOO_LARGE,
        "file.too_large",
    )
    .await;
    let mut chunked = raw_upload(&server, url, "Transfer-Encoding: chunked\r\n").await;
    assert!(
        read_headers(&mut chunked)
            .await
            .starts_with("HTTP/1.1 100 Continue")
    );
    chunked
        .write_all(b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n")
        .await
        .unwrap();
    let headers = read_headers(&mut chunked).await;
    assert!(headers.starts_with("HTTP/1.1 413"));
    assert!(headers.contains("application/problem+json"));
    drop(chunked);
    assert_problem(
        complete(&server, &agent, &task, "FL-1").await,
        StatusCode::CONFLICT,
        "agent_file.not_uploaded",
    )
    .await;
    upload_ok(&server, &registered, "fits").await;
    assert_eq!(
        complete(&server, &agent, &task, "FL-1")
            .await
            .json::<Value>()
            .await
            .unwrap()["data"]["size"],
        4
    );
    server.stop().await;
}

#[tokio::test]
async fn upload_file_fails_when_url_is_expired() {
    let server = Server::configured(|config| config.upload_url_lifetime_seconds = 1).await;
    let (_, agent, task) = prepare(&server).await;
    let registered = register(&server, &agent, &task).await;
    let url = registered["upload_url"].as_str().unwrap();
    let altered = format!("{url}x");
    let mut stream = raw_upload(&server, &altered, "Content-Length: 1000\r\n").await;
    assert!(read_headers(&mut stream).await.starts_with("HTTP/1.1 403"));
    drop(stream);
    assert_problem(
        server
            .client
            .put(&altered)
            .header("If-None-Match", "*")
            .body("bytes")
            .send()
            .await
            .map_err(|_| "upload request failed")
            .unwrap(),
        StatusCode::FORBIDDEN,
        "file.url_invalid",
    )
    .await;
    let other = register(&server, &agent, &task).await;
    let other_url = Url::parse(other["upload_url"].as_str().unwrap()).unwrap();
    let mut changed = Url::parse(url).unwrap();
    changed.set_path(other_url.path());
    assert_problem(
        server
            .client
            .put(changed)
            .header("If-None-Match", "*")
            .body("bytes")
            .send()
            .await
            .map_err(|_| "upload request failed")
            .unwrap(),
        StatusCode::FORBIDDEN,
        "file.url_invalid",
    )
    .await;
    sleep(Duration::from_millis(1100)).await;
    assert_problem(
        upload(&server, &registered, "bytes").await,
        StatusCode::FORBIDDEN,
        "file.url_expired",
    )
    .await;
    assert_problem(
        complete(&server, &agent, &task, "FL-1").await,
        StatusCode::CONFLICT,
        "agent_file.not_uploaded",
    )
    .await;
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    let fresh = register(&server, &agent, &task).await;
    upload_ok(&server, &fresh, "fresh").await;
    server.stop().await;
}

#[tokio::test]
async fn upload_file_fails_when_too_many_uploads_are_in_progress() {
    let server = Server::configured(|config| config.upload_concurrency_limit = 1).await;
    let (_, agent, task) = prepare(&server).await;
    let first = register(&server, &agent, &task).await;
    let second = register(&server, &agent, &task).await;
    let mut stream = held_upload(&server, &first).await;
    let refused = upload(&server, &second, "wait").await;
    assert_eq!(refused.headers()["Retry-After"], "1");
    assert_problem(
        refused,
        StatusCode::SERVICE_UNAVAILABLE,
        "file.upload_limit",
    )
    .await;
    stream.write_all(b"done").await.unwrap();
    assert!(read_headers(&mut stream).await.starts_with("HTTP/1.1 201"));
    drop(stream);
    upload_ok(&server, &second, "next").await;
    server.stop().await;
}

#[tokio::test]
async fn complete_agent_file_fails_when_file_was_not_uploaded() {
    let server = Server::start().await;
    let (_, agent, task) = prepare(&server).await;
    register(&server, &agent, &task).await;
    assert_problem(
        complete(&server, &agent, &task, "FL-1").await,
        StatusCode::CONFLICT,
        "agent_file.not_uploaded",
    )
    .await;
    for key in ["FL-999", "FL-01", "TK-1"] {
        assert_problem(
            complete(&server, &agent, &task, key).await,
            StatusCode::NOT_FOUND,
            "agent_file.not_found",
        )
        .await;
    }
    let second = create(&server, &agent, SPACE, "Second task").await;
    let agent_file = register(&server, &agent, &second).await;
    assert_eq!(agent_file["key"], "FL-2");
    assert_problem(
        complete(&server, &agent, &task, "FL-2").await,
        StatusCode::NOT_FOUND,
        "agent_file.not_found",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn register_agent_file_fails_with_reopen_hint_when_task_is_ended() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Paused file work").await;
    let registered = register(&server, &agent, &task).await;
    let ended = set_status(&server, &agent, &task, "done").await;
    let refusal = assert_problem(
        registration(
            &server,
            &agent,
            SPACE,
            task["task_id"].as_str().unwrap(),
            json!({"file_name":"rejected.txt", "summary":"Rejected upload"}),
        )
        .await,
        StatusCode::CONFLICT,
        "task.state_invalid",
    )
    .await;
    assert_eq!(
        refusal["title"],
        "Operation is not allowed while the task is done; claim or reopen it first"
    );
    assert_eq!(read(&server, &person, "TK-1").await, ended);
    upload_ok(&server, &registered, "Before ending").await;
    let response = complete(&server, &agent, &task, "FL-1").await;
    assert_eq!(response.status(), StatusCode::OK);
    let completed = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(completed["key"], "FL-1");
    assert_eq!(completed["task_title"], "Paused file work");
    assert_eq!(completed["size"], 13);
    let after = read(&server, &person, "TK-1").await;
    assert_eq!(after["state"], "done");
    assert_eq!(after["ended_at"], ended["ended_at"]);
    assert_eq!(after["files"][0]["key"], "FL-1");
    assert_eq!(after["files"][0]["completed_at"], completed["completed_at"]);
    let reopened = set_status(&server, &agent, &task, "working").await;
    assert_eq!(reopened["files"], after["files"]);
    assert_eq!(read(&server, &person, "TK-1").await, reopened);
    assert_eq!(register(&server, &agent, &task).await["key"], "FL-2");
    server.stop().await;
}

#[tokio::test]
async fn register_agent_file_fails_when_caller_is_person() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Agent only").await;
    let bob = login(&server, "bob").await;
    for tokens in [&person, &bob] {
        for space in [SPACE, "SPACE-999"] {
            assert_problem(
                registration(
                    &server,
                    tokens,
                    space,
                    task["task_id"].as_str().unwrap(),
                    json!({"file_name":"a", "summary":"b"}),
                )
                .await,
                StatusCode::FORBIDDEN,
                "auth.agent_required",
            )
            .await;
            assert_problem(
                completion(
                    &server,
                    tokens,
                    space,
                    task["task_id"].as_str().unwrap(),
                    "FL-1",
                )
                .await,
                StatusCode::FORBIDDEN,
                "auth.agent_required",
            )
            .await;
        }
    }
    server.stop().await;
}

#[tokio::test]
async fn register_agent_file_fails_when_file_name_is_too_long() {
    let server = Server::start().await;
    let (_, agent, task) = prepare(&server).await;
    for (file, summary, field, code) in [
        ("a".repeat(256), "Valid".to_owned(), "file_name", "too_long"),
        (String::new(), "Valid".to_owned(), "file_name", "required"),
        ("a".to_owned(), " ".to_owned(), "summary", "required"),
        ("a".to_owned(), "x".repeat(501), "summary", "too_long"),
        (
            "\u{754c}".repeat(86),
            "Valid".to_owned(),
            "file_name",
            "too_long",
        ),
    ] {
        let problem = assert_problem(
            registration(
                &server,
                &agent,
                SPACE,
                task["task_id"].as_str().unwrap(),
                json!({"file_name":file, "summary":summary}),
            )
            .await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
        assert_eq!(problem["errors"][0]["field"], field);
        assert_eq!(problem["errors"][0]["code"], code);
    }
    let response = registration(
        &server,
        &agent,
        SPACE,
        task["task_id"].as_str().unwrap(),
        json!({"file_name":"\u{754c}".repeat(85), "summary":"\u{754c}".repeat(500)}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let registered = response.json::<Value>().await.unwrap()["data"].clone();
    upload_ok(&server, &registered, "").await;
    assert_eq!(
        complete(&server, &agent, &task, "FL-1")
            .await
            .json::<Value>()
            .await
            .unwrap()["data"]["size"],
        0
    );
    server.stop().await;
}

#[tokio::test]
async fn register_agent_file_fails_when_task_belongs_to_another_party() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Alice task").await;
    register(&server, &agent, &task).await;
    let bob = login(&server, "bob").await;
    let bob_agent = authorize_for(&server, &bob, "Bob agent").await;
    space_member(&server, &person, Method::POST, "contributor").await;
    assert_refusals(
        &server,
        &bob_agent,
        SPACE,
        &task,
        (StatusCode::FORBIDDEN, "space.permission_required"),
    )
    .await;
    let own = create(&server, &bob_agent, SPACE, "Bob task").await;
    space_member(&server, &person, Method::PATCH, "viewer").await;
    assert_refusals(
        &server,
        &bob_agent,
        SPACE,
        &own,
        (StatusCode::FORBIDDEN, "space.permission_required"),
    )
    .await;
    space_member(&server, &person, Method::DELETE, "").await;
    create_space(&server, &bob, "Other", "Bob space").await;
    assert_refusals(
        &server,
        &bob_agent,
        "SPACE-2",
        &task,
        (StatusCode::NOT_FOUND, "task.not_found"),
    )
    .await;
    assert_refusals(
        &server,
        &bob_agent,
        "SPACE-2",
        &own,
        (StatusCode::NOT_FOUND, "task.not_found"),
    )
    .await;
    assert_refusals(
        &server,
        &bob_agent,
        SPACE,
        &task,
        (StatusCode::NOT_FOUND, "space.not_found"),
    )
    .await;
    server.stop().await;
}

async fn assert_refusals(
    server: &Server,
    tokens: &Value,
    space: &str,
    task: &Value,
    expected: (StatusCode, &str),
) {
    let (status, code) = expected;
    let id = task["task_id"].as_str().unwrap();
    assert_problem(
        registration(
            server,
            tokens,
            space,
            id,
            json!({"file_name":"a", "summary":"b"}),
        )
        .await,
        status,
        code,
    )
    .await;
    assert_problem(
        completion(server, tokens, space, id, "FL-1").await,
        status,
        code,
    )
    .await;
}

#[tokio::test]
async fn complete_agent_file_fails_with_not_found_when_registration_was_abandoned() {
    let server = Server::configured(|config| config.file_cleanup_interval_seconds = 1).await;
    let (person, agent, task) = prepare(&server).await;
    let abandoned = register(&server, &agent, &task).await;
    let completed = register(&server, &agent, &task).await;
    upload_ok(&server, &abandoned, "Abandoned file").await;
    upload_ok(&server, &completed, "Complete file").await;
    assert_eq!(
        complete(&server, &agent, &task, "FL-2").await.status(),
        StatusCode::OK
    );
    assert_problem(
        upload(&server, &abandoned, "Retry").await,
        StatusCode::PRECONDITION_FAILED,
        "file.already_exists",
    )
    .await;
    age_registration(&server, "FL-2").await;
    age_registration(&server, "FL-1").await;
    timeout(TEST_TIMEOUT, async {
        loop {
            let response = upload(&server, &abandoned, "Retry").await;
            if response.status() == StatusCode::PRECONDITION_FAILED {
                sleep(Duration::from_millis(20)).await;
                continue;
            }
            assert_eq!(response.status(), StatusCode::CREATED);
            break;
        }
    })
    .await
    .unwrap();
    assert_problem(
        complete(&server, &agent, &task, "FL-1").await,
        StatusCode::NOT_FOUND,
        "agent_file.not_found",
    )
    .await;
    let fresh = register(&server, &agent, &task).await;
    assert_eq!(fresh["key"], "FL-3");
    upload_ok(&server, &fresh, "Fresh file").await;
    assert_eq!(
        complete(&server, &agent, &task, "FL-3").await.status(),
        StatusCode::OK
    );
    let read_back = read(&server, &person, "TK-1").await;
    assert_eq!(
        read_back["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|agent_file| agent_file["key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["FL-2", "FL-3"]
    );
    server.stop().await;
}

#[tokio::test]
async fn upload_file_releases_slot_when_upload_stalls() {
    let server = Server::configured(|config| {
        config.upload_concurrency_limit = 1;
        config.upload_idle_timeout_seconds = 3;
    })
    .await;
    let (_, agent, task) = prepare(&server).await;
    let first = register(&server, &agent, &task).await;
    let second = register(&server, &agent, &task).await;
    let mut stalled = raw_upload(
        &server,
        first["upload_url"].as_str().unwrap(),
        "Content-Length: 4\r\nConnection: close\r\n",
    )
    .await;
    assert!(
        read_headers(&mut stalled)
            .await
            .starts_with("HTTP/1.1 100 Continue")
    );
    stalled.write_all(b"a").await.unwrap();
    assert_problem(
        upload(&server, &second, "next").await,
        StatusCode::SERVICE_UNAVAILABLE,
        "file.upload_limit",
    )
    .await;
    let mut response = String::new();
    timeout(TEST_TIMEOUT, stalled.read_to_string(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(response.starts_with("HTTP/1.1 400"));
    assert!(response.contains("application/problem+json"));
    assert_eq!(
        serde_json::from_str::<Value>(response.split_once("\r\n\r\n").unwrap().1).unwrap()["code"],
        "file.interrupted"
    );
    upload_ok(&server, &second, "next").await;
    upload_ok(&server, &first, "done").await;
    assert_eq!(
        complete(&server, &agent, &task, "FL-1").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        complete(&server, &agent, &task, "FL-2").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        read(&server, &agent, "TK-1").await["files"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    server.stop().await;
}

#[tokio::test]
async fn complete_agent_file_fails_when_file_exceeds_size_limit() {
    let server = Server::start().await;
    let (person, agent, task) = prepare(&server).await;
    let registration = register(&server, &agent, &task).await;
    upload_ok(&server, &registration, "12345").await;

    let directory = server.stop().await;
    let server =
        Server::restart_configured(directory, |config| config.upload_size_limit_bytes = 4).await;
    assert_problem(
        complete(&server, &agent, &task, "FL-1").await,
        StatusCode::CONFLICT,
        "agent_file.too_large",
    )
    .await;
    assert_eq!(read(&server, &person, "TK-1").await["files"], json!([]));
    assert_problem(
        request(
            &server,
            &person,
            Method::GET,
            "/api/v1/spaces/SPACE-1/files/FL-1/url",
            Value::Null,
        )
        .await,
        StatusCode::NOT_FOUND,
        "agent_file.not_found",
    )
    .await;
    assert_problem(
        complete(&server, &agent, &task, "FL-1").await,
        StatusCode::CONFLICT,
        "agent_file.refused",
    )
    .await;
    server.stop().await;
}

#[cfg(unix)]
#[tokio::test]
async fn orphan_cleanup_removes_old_unreferenced_regular_files() {
    let server = Server::configured(|config| config.file_cleanup_interval_seconds = 1).await;
    let (person, agent, task) = prepare(&server).await;
    let registration = register(&server, &agent, &task).await;
    upload_ok(&server, &registration, "Referenced").await;
    assert_eq!(
        complete(&server, &agent, &task, "FL-1").await.status(),
        StatusCode::OK
    );
    let stored = server.directory.path().join("data/agent_files");
    let referenced = fs::read_dir(&stored)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let old_times = FileTimes::new().set_modified(UNIX_EPOCH);
    File::open(&referenced)
        .unwrap()
        .set_times(old_times)
        .unwrap();
    let old_files = [
        stored.join("41000000-0000-7000-8000-000000000001"),
        stored.join("old-orphan.txt"),
        stored.join(OsStr::from_bytes(b"orphan-\xff")),
    ];
    for orphan in &old_files {
        File::create(orphan).unwrap().set_times(old_times).unwrap();
    }
    let young = stored.join("young-orphan.txt");
    fs::write(&young, b"Young").unwrap();
    let link = stored.join("orphan-link");
    symlink(&referenced, &link).unwrap();
    timeout(TEST_TIMEOUT, async {
        while old_files.iter().any(|path| path.exists()) {
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(fs::read(young).unwrap(), b"Young");
    assert!(fs::symlink_metadata(link).unwrap().is_symlink());
    assert_eq!(fs::read(referenced).unwrap(), b"Referenced");
    assert_eq!(
        read(&server, &person, "TK-1").await["files"][0]["key"],
        "FL-1"
    );
    server.stop().await;
}
