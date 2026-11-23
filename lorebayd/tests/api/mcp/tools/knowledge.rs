use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::helpers::{
    Server, TEST_PASSWORD, authorize_for, create_space, list_page, login, request,
};
use crate::knowledge::helpers::{self as knowledge, PATH};
use crate::proposals::helpers as proposals;
use crate::tasks::helpers::{self as tasks, SPACE};

use super::{call, call_error};

#[tokio::test]
async fn call_list_knowledge_returns_top_level_when_prefix_is_missing() {
    let (server, person, agent) = tasks::setup().await;
    knowledge::create(&server, &person, "Guide", "Overview", "Start here").await;
    knowledge::create(&server, &person, "Team/Build", "Build", "Commands").await;
    knowledge::create(&server, &person, "Team/Deploy", "Deploy", "Steps").await;
    let mut arguments = json!({"space":SPACE, "page_size":1});
    let first = call(&server, &agent, "list_knowledge", arguments.clone()).await;
    assert_eq!(
        first,
        list_page(&server, &person, PATH, &[("page_size", "1")]).await
    );
    assert_eq!(
        first["data"],
        json!([{"name":"Guide", "key":"KN-1", "summary":"Overview"}])
    );
    arguments["cursor"] = first["meta"]["next_cursor"].clone();
    let second = call(&server, &agent, "list_knowledge", arguments).await;
    assert_eq!(second, json!({"data":[{"name":"Team/"}], "meta":{}}));
    let nested = call(
        &server,
        &agent,
        "list_knowledge",
        json!({"space":SPACE, "prefix":"Team/"}),
    )
    .await;
    assert_eq!(
        nested,
        list_page(&server, &person, PATH, &[("prefix", "Team/")]).await
    );
    assert_eq!(nested["data"].as_array().unwrap().len(), 2);
    for extra in [
        json!({"unknown":"refused"}),
        json!({"prefix":false}),
        json!({"page_size":0}),
        json!({"cursor":"invalid"}),
    ] {
        let mut arguments = json!({"space":SPACE});
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(
            call_error(&server, &agent, "list_knowledge", arguments).await["code"]
                .as_str()
                .unwrap()
                .starts_with("request.invalid_")
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn call_list_knowledge_searches_space_when_keyword_is_given() {
    let (server, person, agent) = tasks::setup().await;
    knowledge::create(&server, &person, "Nested/Needle", "Summary", "Body").await;
    knowledge::create(&server, &person, "Other", "Needle summary", "Body").await;
    knowledge::create(&server, &person, "Third", "Summary", "Needle body").await;
    knowledge::create(&server, &person, "Unrelated", "Summary", "Body").await;
    let mut arguments = json!({"space":SPACE, "keyword":"NEEDLE", "page_size":1});
    let mut names = Vec::new();
    loop {
        let page = call(&server, &agent, "list_knowledge", arguments.clone()).await;
        names.push(page["data"][0]["name"].clone());
        let Some(cursor) = page["meta"]["next_cursor"].as_str() else {
            break;
        };
        assert!(names.len() < 3);
        arguments["cursor"] = json!(cursor);
    }
    assert_eq!(
        names,
        json!(["Nested/Needle", "Other", "Third"])
            .as_array()
            .unwrap()
            .clone()
    );
    for extra in [
        json!({"prefix":"Nested/", "keyword":"Needle"}),
        json!({"keyword":""}),
        json!({"keyword":"x".repeat(201)}),
    ] {
        let mut arguments = json!({"space":SPACE});
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert_eq!(
            call_error(&server, &agent, "list_knowledge", arguments).await["code"],
            "request.invalid_query"
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn call_read_knowledge_returns_latest_version_by_name_or_key() {
    let (server, person, agent) = tasks::setup().await;
    knowledge::create(&server, &person, "Old", "Original", "Version one").await;
    let latest = proposals::change_entry(
        &server,
        &person,
        "KN-1",
        json!({"name":"Team/Guide", "summary":"Current", "content":"Version two"}),
    )
    .await;
    assert_eq!(latest["version"], 2);
    for target in [json!({"name":"team/GUIDE"}), json!({"key":"KN-1"})] {
        let mut arguments = target;
        arguments["space"] = json!(SPACE);
        assert_eq!(
            call(&server, &agent, "read_knowledge", arguments).await,
            latest
        );
    }
    for target in [
        json!({"name":"Team/Guide", "key":"KN-1"}),
        json!({}),
        json!({"name":false}),
        json!({"key":"KN-1", "unknown":"refused"}),
        json!({"name":"x".repeat(201)}),
    ] {
        let mut arguments = target;
        arguments["space"] = json!(SPACE);
        assert_eq!(
            call_error(&server, &agent, "read_knowledge", arguments).await["code"],
            "request.invalid_body"
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn call_read_knowledge_fails_when_entry_is_deleted() {
    let (server, person, agent) = tasks::setup().await;
    knowledge::create(&server, &person, "Guide", "Summary", "Content").await;
    proposals::delete_entry(&server, &person, "KN-1").await;
    for target in [json!({"name":"Guide"}), json!({"key":"KN-1"})] {
        let mut arguments = target;
        arguments["space"] = json!(SPACE);
        assert_eq!(
            call_error(&server, &agent, "read_knowledge", arguments).await["code"],
            "knowledge.not_found"
        );
    }
    assert_eq!(
        call(&server, &agent, "list_knowledge", json!({"space":SPACE})).await,
        json!({"data":[], "meta":{}})
    );
    server.stop().await;
}

#[tokio::test]
async fn call_read_knowledge_returns_approved_content_to_another_agent() {
    let (server, owner, bob, charlie) = three_people_with_space().await;
    let writer = authorize_for(&server, &owner, "Proposing agent").await;
    let reader = authorize_for(&server, &charlie, "Reading agent").await;
    let task = call(
        &server,
        &writer,
        "create_task",
        json!({"space":SPACE, "title":"Share team convention"}),
    )
    .await;
    let mut arguments = proposals::creation("Team/Convention");
    arguments["task_id"] = task["task_id"].clone();
    let proposal = call(&server, &writer, "create_proposal", arguments).await;
    assert_eq!(proposal["status"], "pending");
    assert_eq!(
        call_error(
            &server,
            &reader,
            "read_knowledge",
            json!({"space":SPACE, "name":"Team/Convention"})
        )
        .await["code"],
        "knowledge.not_found"
    );
    let approved = proposals::approve(&server, &bob, "PP-1").await;
    assert_eq!(approved["status"], "approved");
    let entry = call(
        &server,
        &reader,
        "read_knowledge",
        json!({"space":SPACE, "name":"Team/Convention"}),
    )
    .await;
    assert_eq!(entry, proposals::entry(&server, &charlie, "KN-1").await);
    assert_eq!(entry["content"], "Content");
    assert_eq!(entry["applied_by"], "bob");
    assert_eq!(
        proposals::versions(&server, &charlie, "KN-1").await[0]["proposal_key"],
        "PP-1"
    );
    server.stop().await;
}

async fn three_people_with_space() -> (Server, Value, Value, Value) {
    let server = Server::restart_with_users(
        TempDir::new().unwrap(),
        &[
            ("alice", TEST_PASSWORD),
            ("bob", TEST_PASSWORD),
            ("charlie", TEST_PASSWORD),
        ],
    )
    .await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared").await;
    for person in ["bob", "charlie"] {
        proposals::data(
            request(
                &server,
                &owner,
                Method::POST,
                &format!("/api/v1/spaces/{SPACE}/members/{person}"),
                json!({"role":"contributor"}),
            )
            .await,
            StatusCode::CREATED,
        )
        .await;
    }
    let bob = login(&server, "bob").await;
    let charlie = login(&server, "charlie").await;
    (server, owner, bob, charlie)
}
