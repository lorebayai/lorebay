use std::time::Duration;

use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};
use tokio::time::sleep;
use url::Url;

use crate::helpers::{Server, assert_problem, authorize_for, create_space, login, request};
use crate::tasks::helpers::{SPACE, create, participant, read, setup};

use super::helpers::{complete, file_url, prepare, register, registration, upload_ok};

#[tokio::test]
async fn get_artifact_download_url_returns_url_that_serves_uploaded_bytes() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Shared file").await;
    let artifact = uploaded(&server, &agent, &task, "report.txt").await;
    let bob = login(&server, "bob").await;
    participant(&server, &person, Method::POST, "viewer").await;
    let reader = authorize_for(&server, &bob, "Reader").await;
    let before = read(&server, &person, "TK-1").await;
    for caller in [&person, &bob, &reader] {
        let download = download_url(&server, caller, SPACE, &artifact).await;
        let url = Url::parse(download["url"].as_str().unwrap()).unwrap();
        assert_eq!(url.origin().ascii_serialization(), server.public_url);
        let expiry = url
            .query_pairs()
            .find(|(key, _)| key == "expires")
            .unwrap()
            .1;
        assert_eq!(expiry.parse::<i64>().unwrap(), download["expires_at"]);
        let response = fetch(&server, download["url"].as_str().unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-length"], "14");
        assert_security(&response, "sandbox");
        assert_eq!(response.bytes().await.unwrap(), "Uploaded bytes");
    }
    assert_eq!(read(&server, &person, "TK-1").await, before);
    participant(&server, &person, Method::DELETE, "").await;
    for caller in [&bob, &reader] {
        assert_problem(
            url_request(&server, caller, SPACE, &artifact).await,
            StatusCode::NOT_FOUND,
            "space.not_found",
        )
        .await;
    }
    server.stop().await;
}

#[tokio::test]
async fn download_file_is_served_as_attachment_when_file_is_html_or_svg() {
    let server = Server::start().await;
    let (person, agent, task) = prepare(&server).await;
    for name in [
        "page.html",
        "image.SVG",
        "a\"\r\nX-Injected: yes.html",
        "\u{754c}\\100%.txt",
    ] {
        let artifact = uploaded(&server, &agent, &task, name).await;
        let download = download_url(&server, &person, SPACE, &artifact).await;
        let response = fetch(&server, download["url"].as_str().unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"],
            "application/octet-stream"
        );
        assert_security(&response, "sandbox");
        assert!(!response.headers().contains_key("x-injected"));
        let disposition = response.headers()["content-disposition"].to_str().unwrap();
        assert!(disposition.starts_with("attachment; filename=\""));
        assert!(!disposition.chars().any(char::is_control));
        if name.contains('"') {
            assert_eq!(
                disposition,
                "attachment; filename=\"a\\\"X-Injected: yes.html\"; filename*=UTF-8''a%22X-Injected%3A%20yes.html"
            );
        }
        if name.starts_with('\u{754c}') {
            assert_eq!(
                disposition,
                "attachment; filename=\"_\\\\100%.txt\"; filename*=UTF-8''%E7%95%8C%5C100%25.txt"
            );
        }
        assert_eq!(response.bytes().await.unwrap(), "Uploaded bytes");
    }
    server.stop().await;
}

#[tokio::test]
async fn download_file_opens_inline_when_file_is_png_or_pdf() {
    let server = Server::start().await;
    let (person, agent, task) = prepare(&server).await;
    for (extension, content_type) in [
        ("png", "image/png"),
        ("PNG", "image/png"),
        ("pdf", "application/pdf"),
        ("PDF", "application/pdf"),
        ("jpg", "image/jpeg"),
        ("JPEG", "image/jpeg"),
        ("gif", "image/gif"),
        ("WEBP", "image/webp"),
    ] {
        let name = format!("file.{extension}");
        let artifact = uploaded(&server, &agent, &task, &name).await;
        let download = download_url(&server, &person, SPACE, &artifact).await;
        let response = fetch(&server, download["url"].as_str().unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], content_type);
        assert_eq!(
            response.headers()["content-disposition"],
            format!("inline; filename=\"{name}\"; filename*=UTF-8''{name}")
        );
        assert_security(
            &response,
            if content_type == "application/pdf" {
                ""
            } else {
                "sandbox"
            },
        );
        assert_eq!(response.bytes().await.unwrap(), "Uploaded bytes");
    }
    server.stop().await;
}

#[tokio::test]
async fn get_artifact_download_url_fails_when_artifact_is_in_another_space() {
    let server = Server::start().await;
    let (person, agent, task) = prepare(&server).await;
    let artifact = uploaded(&server, &agent, &task, "a.txt").await;
    create_space(&server, &person, "Other", "Other space").await;
    for (space, key, code) in [
        ("SPACE-2", artifact.as_str(), "artifact.not_found"),
        (SPACE, "AR-999", "artifact.not_found"),
        (SPACE, "AR-01", "artifact.not_found"),
        (SPACE, "TK-1", "artifact.not_found"),
        ("SPACE-999", artifact.as_str(), "space.not_found"),
    ] {
        assert_problem(
            url_request(&server, &person, space, key).await,
            StatusCode::NOT_FOUND,
            code,
        )
        .await;
    }
    server.stop().await;
}

#[tokio::test]
async fn get_artifact_download_url_fails_when_upload_is_not_complete() {
    let server = Server::start().await;
    let (person, agent, task) = prepare(&server).await;
    let registered = register(&server, &agent, &task).await;
    for uploaded_bytes in [false, true] {
        if uploaded_bytes {
            upload_ok(&server, &registered, "Uploaded bytes").await;
        }
        assert_problem(
            url_request(&server, &person, SPACE, "AR-1").await,
            StatusCode::NOT_FOUND,
            "artifact.not_found",
        )
        .await;
    }
    assert_eq!(
        complete(&server, &agent, &task, "AR-1").await.status(),
        StatusCode::OK
    );
    download_url(&server, &person, SPACE, "AR-1").await;
    server.stop().await;
}

#[tokio::test]
async fn download_file_fails_when_url_is_expired() {
    let server = Server::start().await;
    let (person, agent, task) = prepare(&server).await;
    let artifact = uploaded(&server, &agent, &task, "a.txt").await;
    let upload = register(&server, &agent, &task).await;
    let directory = server.stop().await;
    let server =
        Server::restart_configured(directory, |config| config.upload_url_lifetime_seconds = 1)
            .await;
    let download = download_url(&server, &person, SPACE, &artifact).await;
    assert_problem(
        fetch(&server, upload["upload_url"].as_str().unwrap()).await,
        StatusCode::FORBIDDEN,
        "file.url_invalid",
    )
    .await;
    let mut upload_url = Url::parse(upload["upload_url"].as_str().unwrap()).unwrap();
    upload_url
        .query_pairs_mut()
        .append_pair("file_name", "a.txt");
    assert_problem(
        fetch(&server, upload_url.as_str()).await,
        StatusCode::FORBIDDEN,
        "file.url_invalid",
    )
    .await;
    sleep(Duration::from_millis(1100)).await;
    let response = fetch(&server, download["url"].as_str().unwrap()).await;
    assert_security(&response, "sandbox");
    assert_problem(response, StatusCode::FORBIDDEN, "file.url_expired").await;
    server.stop().await;
}

#[tokio::test]
async fn download_file_fails_when_url_was_altered() {
    let server = Server::start().await;
    let (person, agent, task) = prepare(&server).await;
    let artifact = uploaded(&server, &agent, &task, "a.html").await;
    let download = download_url(&server, &person, SPACE, &artifact).await;
    let url = Url::parse(download["url"].as_str().unwrap()).unwrap();
    let other = uploaded(&server, &agent, &task, "b.pdf").await;
    let other = download_url(&server, &person, SPACE, &other).await;
    let mut changed_path = url.clone();
    changed_path.set_path(Url::parse(other["url"].as_str().unwrap()).unwrap().path());
    let mut altered = vec![changed_path];
    for field in ["expires", "signature", "file_name"] {
        let mut changed = url.clone();
        let pairs: Vec<_> = url
            .query_pairs()
            .map(|(key, value)| {
                let value = if key == field {
                    format!("{value}0")
                } else {
                    value.into_owned()
                };
                (key.into_owned(), value)
            })
            .collect();
        changed.query_pairs_mut().clear().extend_pairs(pairs);
        altered.push(changed);
    }
    altered.push(replace_query(&url, "file_name", "a.pdf"));
    for changed in altered {
        assert_problem(
            fetch(&server, changed.as_str()).await,
            StatusCode::FORBIDDEN,
            "file.url_invalid",
        )
        .await;
    }
    let pairs: Vec<_> = url
        .query_pairs()
        .filter(|(key, _)| key != "file_name")
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    let mut upload = url;
    upload.query_pairs_mut().clear().extend_pairs(pairs);
    assert_problem(
        server
            .client
            .put(upload)
            .header("If-None-Match", "*")
            .body("overwrite")
            .send()
            .await
            .map_err(|_| "file request failed")
            .unwrap(),
        StatusCode::FORBIDDEN,
        "file.url_invalid",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn download_file_fails_when_artifact_was_deleted() {
    let (server, person, agent) = setup().await;
    for resource in ["artifact", "task"] {
        let task = create(&server, &agent, SPACE, "Deleted download").await;
        let registered = register(&server, &agent, &task).await;
        let key = registered["key"].as_str().unwrap();
        upload_ok(&server, &registered, "Uploaded bytes").await;
        assert_eq!(
            complete(&server, &agent, &task, key).await.status(),
            StatusCode::OK
        );
        let download = download_url(&server, &person, SPACE, key).await;
        let url = download["url"].as_str().unwrap();
        let response = fetch(&server, url).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.bytes().await.unwrap(), "Uploaded bytes");
        let mut path = format!(
            "/api/v1/spaces/{SPACE}/tasks/{}",
            task["task_id"].as_str().unwrap()
        );
        if resource == "artifact" {
            path.push_str(&format!("/artifacts/{key}"));
        }
        let response = request(&server, &person, Method::DELETE, &path, Value::Null).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({"data":null,"meta":{}})
        );
        assert_deleted_download(&server, url).await;
        upload_ok(&server, &registered, "Leftover file bytes").await;
        assert_deleted_download(&server, url).await;
        assert_problem(
            url_request(&server, &person, SPACE, key).await,
            StatusCode::NOT_FOUND,
            "artifact.not_found",
        )
        .await;
    }
    server.stop().await;
}

async fn assert_deleted_download(server: &Server, url: &str) {
    let response = fetch(server, url).await;
    assert_security(&response, "sandbox");
    assert_problem(response, StatusCode::NOT_FOUND, "file.not_found").await;
    let response = server
        .client
        .head(file_url(server, url))
        .send()
        .await
        .map_err(|_| "download request failed")
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_security(&response, "sandbox");
    assert!(response.bytes().await.unwrap().is_empty());
}

async fn uploaded(server: &Server, agent: &Value, task: &Value, name: &str) -> String {
    let response = registration(
        server,
        agent,
        SPACE,
        task["task_id"].as_str().unwrap(),
        json!({"file_name": name, "summary": "Download fixture"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let registered = response.json::<Value>().await.unwrap()["data"].clone();
    upload_ok(server, &registered, "Uploaded bytes").await;
    let key = registered["key"].as_str().unwrap();
    assert_eq!(
        complete(server, agent, task, key).await.status(),
        StatusCode::OK
    );
    key.to_owned()
}

async fn url_request(server: &Server, caller: &Value, space: &str, key: &str) -> Response {
    request(
        server,
        caller,
        Method::GET,
        &format!("/api/v1/spaces/{space}/artifacts/{key}/url"),
        Value::Null,
    )
    .await
}

async fn download_url(server: &Server, caller: &Value, space: &str, key: &str) -> Value {
    let response = url_request(server, caller, space, key).await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["data"].clone()
}

async fn fetch(server: &Server, url: &str) -> Response {
    server
        .client
        .get(file_url(server, url))
        .send()
        .await
        .map_err(|_| "download request failed")
        .unwrap()
}

fn assert_security(response: &Response, policy: &str) {
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    if policy.is_empty() {
        assert!(!response.headers().contains_key("content-security-policy"));
    } else {
        assert_eq!(response.headers()["content-security-policy"], policy);
    }
}

fn replace_query(url: &Url, field: &str, replacement: &str) -> Url {
    let mut changed = url.clone();
    let pairs: Vec<_> = url
        .query_pairs()
        .map(|(key, value)| {
            let value = if key == field {
                replacement.to_owned()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect();
    changed.query_pairs_mut().clear().extend_pairs(pairs);
    changed
}
