use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{
    assert_problem, authorize_for, create_space, list, list_page, list_response, login, request,
    two_people, walk,
};
use crate::tasks::helpers::participant;

use helpers::*;

pub(crate) mod helpers;

#[tokio::test]
async fn list_knowledge_versions_returns_every_version_with_who_applied_it() {
    let server = two_people().await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared").await;
    participant(&server, &owner, Method::POST, "contributor").await;
    participant(&server, &owner, Method::PATCH, "admin").await;
    let bob = login(&server, "bob").await;
    let first = seed(&server, &owner, "Guide").await;
    let second = change(&server, &bob, &by_key("KN-1"), json!({"summary":"Updated"})).await;
    let third = delete(&server, &owner, &by_key("KN-1")).await;
    seed(&server, &owner, "Other").await;
    let path = resource(&by_key("KN-1"), "versions");
    let expected = vec![
        version_metadata(&third),
        version_metadata(&second),
        version_metadata(&first),
    ];
    for size in ["1", "2", "100"] {
        assert_eq!(walk(&server, &owner, &path, size, 3).await, expected);
    }
    assert_eq!(
        list(&server, &owner, &resource(&by_name("guide"), "versions")).await,
        json!(expected)
    );
    let page = list_page(&server, &owner, &path, &[("page_size", "1")]).await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    for (caller, other) in [
        (&owner, resource(&by_key("KN-2"), "versions")),
        (&bob, path.clone()),
        (&owner, format!("{PATH}/deleted")),
    ] {
        assert_problem(
            list_response(&other, &server, caller, &[("cursor", cursor)]).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
    for query in [
        vec![("cursor", "invalid")],
        vec![("page_size", "0")],
        vec![("page_size", "101")],
    ] {
        assert_problem(
            list_response(&path, &server, &owner, &query).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
}

#[tokio::test]
async fn list_knowledge_versions_succeeds_when_caller_is_viewer() {
    let server = two_people().await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared").await;
    participant(&server, &owner, Method::POST, "viewer").await;
    let bob = login(&server, "bob").await;
    let first = seed(&server, &owner, "Guide").await;
    let deleted = delete(&server, &owner, &by_key("KN-1")).await;
    assert_eq!(
        list(&server, &bob, &resource(&by_name("guide"), "versions")).await,
        json!([version_metadata(&deleted), version_metadata(&first)])
    );
    assert_eq!(
        read(&server, &bob, &resource(&by_key("KN-1"), "versions/1")).await,
        first
    );
    assert_eq!(
        list(&server, &bob, &format!("{PATH}/deleted")).await[0]["key"],
        "KN-1"
    );
    create_space(&server, &owner, "Private", "Other space").await;
    let agent = authorize_for(&server, &bob, "Reader").await;
    for caller in [&bob, &agent] {
        assert_history_target_error(
            &server,
            caller,
            "/api/v1/spaces/SPACE-2/knowledge/by-key/KN-1",
            StatusCode::NOT_FOUND,
            "space.not_found",
        )
        .await;
        assert_problem(
            request(
                &server,
                caller,
                Method::GET,
                "/api/v1/spaces/SPACE-2/knowledge/deleted",
                Value::Null,
            )
            .await,
            StatusCode::NOT_FOUND,
            "space.not_found",
        )
        .await;
    }
}

#[tokio::test]
async fn list_knowledge_versions_fails_when_caller_is_agent() {
    let (server, owner) = setup().await;
    let first = seed(&server, &owner, "Guide").await;
    let agent = authorize_for(&server, &owner, "Reader").await;
    for target in [by_key("KN-1"), by_name("Guide")] {
        for suffix in ["versions", "versions/1"] {
            assert_problem(
                request(
                    &server,
                    &agent,
                    Method::GET,
                    &resource(&target, suffix),
                    Value::Null,
                )
                .await,
                StatusCode::FORBIDDEN,
                "auth.person_required",
            )
            .await;
        }
        assert_problem(
            request(
                &server,
                &agent,
                Method::POST,
                &resource(&target, "rollback"),
                json!({"version":1}),
            )
            .await,
            StatusCode::FORBIDDEN,
            "auth.person_required",
        )
        .await;
    }
    assert_eq!(read(&server, &agent, &by_key("KN-1")).await, first);
    delete(&server, &owner, &by_key("KN-1")).await;
    assert_problem(
        request(
            &server,
            &agent,
            Method::GET,
            &resource(&by_key("KN-1"), "versions/1"),
            Value::Null,
        )
        .await,
        StatusCode::FORBIDDEN,
        "auth.person_required",
    )
    .await;
}

#[tokio::test]
async fn read_knowledge_version_returns_old_content() {
    let (server, owner) = setup().await;
    let name = "docs/KN-2 #?&+.md";
    let first = seed(&server, &owner, name).await;
    let second = change(
        &server,
        &owner,
        &by_key("KN-1"),
        json!({"summary":"Changed","content":"New"}),
    )
    .await;
    for target in [by_key("KN-1"), by_name(name)] {
        assert_eq!(
            read(&server, &owner, &resource(&target, "versions/1")).await,
            first
        );
        for version in ["0", "-1", "999"] {
            assert_problem(
                request(
                    &server,
                    &owner,
                    Method::GET,
                    &resource(&target, &format!("versions/{version}")),
                    Value::Null,
                )
                .await,
                StatusCode::NOT_FOUND,
                "knowledge.version_not_found",
            )
            .await;
        }
    }
    for target in [by_key("KN-99"), by_name("Missing")] {
        assert_history_target_error(
            &server,
            &owner,
            &target,
            StatusCode::NOT_FOUND,
            "knowledge.not_found",
        )
        .await;
    }
    for target in [
        format!("{PATH}/entry"),
        format!("{}?name=Guide", by_key("KN-1")),
    ] {
        assert_history_target_error(
            &server,
            &owner,
            &target,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, second);
}

#[tokio::test]
async fn read_knowledge_version_returns_deletion_version() {
    let (server, owner) = setup().await;
    let first = seed(&server, &owner, "Guide").await;
    let deleted = delete(&server, &owner, &by_name("Guide")).await;
    for target in [by_key("KN-1"), by_name("guide")] {
        let old = read(&server, &owner, &resource(&target, "versions/2")).await;
        assert_eq!(old, deleted);
        assert_eq!(old["kind"], "deletion");
        assert_eq!(old["applied_by"], "alice");
        assert!(old["applied_at"].as_i64().unwrap() > 0);
        assert!(old["summary"].is_null());
        assert!(old["content"].is_null());
        assert_eq!(
            read(&server, &owner, &resource(&target, "versions/1")).await,
            first
        );
    }
}

#[tokio::test]
async fn list_deleted_knowledge_returns_deleted_entries_only() {
    let server = two_people().await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared").await;
    participant(&server, &owner, Method::POST, "contributor").await;
    participant(&server, &owner, Method::PATCH, "admin").await;
    let bob = login(&server, "bob").await;
    let path = format!("{PATH}/deleted");
    assert_eq!(list(&server, &owner, &path).await, json!([]));
    for name in ["Zed", "Live", "Alpha", "Restored"] {
        seed(&server, &owner, name).await;
    }
    let third = delete(&server, &owner, &by_key("KN-3")).await;
    let first = delete(&server, &bob, &by_key("KN-1")).await;
    delete(&server, &owner, &by_key("KN-4")).await;
    roll_back(&server, &owner, &by_key("KN-4"), 1).await;
    let expected = json!([
        {"key":"KN-1","name":"Zed","deleted_by":"bob","deleted_at":first["applied_at"]},
        {"key":"KN-3","name":"Alpha","deleted_by":"alice","deleted_at":third["applied_at"]}
    ]);
    for size in ["1", "100"] {
        assert_eq!(json!(walk(&server, &owner, &path, size, 2).await), expected);
    }
    assert_eq!(
        list(&server, &owner, PATH).await.as_array().unwrap().len(),
        2
    );
    let page = list_page(&server, &owner, &path, &[("page_size", "1")]).await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    create_space(&server, &owner, "Second", "Other").await;
    for other in [
        resource(&by_key("KN-1"), "versions"),
        "/api/v1/spaces/SPACE-2/knowledge/deleted".to_owned(),
    ] {
        assert_problem(
            list_response(&other, &server, &owner, &[("cursor", cursor)]).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
}

#[tokio::test]
async fn list_deleted_knowledge_fails_when_caller_is_agent() {
    let (server, owner) = setup().await;
    let agent = authorize_for(&server, &owner, "Reader").await;
    let path = format!("{PATH}/deleted");
    assert_problem(
        request(&server, &agent, Method::GET, &path, Value::Null).await,
        StatusCode::FORBIDDEN,
        "auth.person_required",
    )
    .await;
    seed(&server, &owner, "Guide").await;
    delete(&server, &owner, &by_key("KN-1")).await;
    assert_problem(
        request(&server, &agent, Method::GET, &path, Value::Null).await,
        StatusCode::FORBIDDEN,
        "auth.person_required",
    )
    .await;
    assert_eq!(list(&server, &agent, PATH).await, json!([]));
}

#[tokio::test]
async fn roll_back_knowledge_entry_adds_version_copying_old_content() {
    let server = two_people().await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared").await;
    participant(&server, &owner, Method::POST, "contributor").await;
    participant(&server, &owner, Method::PATCH, "admin").await;
    let bob = login(&server, "bob").await;
    let first = seed(&server, &owner, "Guide").await;
    let second = change(
        &server,
        &owner,
        &by_key("KN-1"),
        json!({"summary":"Changed summary"}),
    )
    .await;
    let third = change(
        &server,
        &owner,
        &by_key("KN-1"),
        json!({"content":"Changed content"}),
    )
    .await;
    let rolled = roll_back(&server, &bob, &by_key("KN-1"), 1).await;
    assert_eq!(rolled["key"], first["key"]);
    assert_eq!(rolled["version"], 4);
    assert_eq!(rolled["content"], first["content"]);
    assert_eq!(rolled["summary"], first["summary"]);
    assert_eq!(rolled["applied_by"], "bob");
    assert!(rolled["applied_at"].as_i64().unwrap() >= third["applied_at"].as_i64().unwrap());
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, rolled);
    for (version, expected) in [(1, first.clone()), (2, second), (3, third)] {
        assert_eq!(
            read(
                &server,
                &owner,
                &resource(&by_key("KN-1"), &format!("versions/{version}"))
            )
            .await,
            expected
        );
    }
    let history = list(&server, &owner, &resource(&by_key("KN-1"), "versions")).await;
    assert_eq!(
        history
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["version"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        vec![4, 3, 2, 1]
    );
    assert!(history[0]["proposal_key"].is_null());
    assert_eq!(
        roll_back(&server, &owner, &by_key("KN-1"), 4).await["version"],
        5
    );
}

#[tokio::test]
async fn roll_back_knowledge_entry_restores_deleted_entry_as_new_version() {
    let (server, owner) = setup().await;
    let first = seed(&server, &owner, "KN-99").await;
    let deleted = delete(&server, &owner, &by_key("KN-1")).await;
    let rolled = roll_back(&server, &owner, &by_name("kn-99"), 1).await;
    assert_eq!(rolled["version"], 3);
    assert_eq!(rolled["kind"], "content");
    assert_eq!(rolled["name"], "KN-99");
    assert_eq!(rolled["content"], first["content"]);
    assert_eq!(rolled["summary"], first["summary"]);
    let agent = authorize_for(&server, &owner, "Reader").await;
    assert_eq!(read(&server, &agent, &by_key("KN-1")).await, rolled);
    assert_eq!(
        list(&server, &owner, &format!("{PATH}/deleted")).await,
        json!([])
    );
    assert_eq!(list(&server, &agent, PATH).await[0]["key"], "KN-1");
    assert_eq!(
        list(&server, &owner, &resource(&by_name("KN-99"), "versions")).await,
        json!([
            version_metadata(&rolled),
            version_metadata(&deleted),
            version_metadata(&first)
        ])
    );
}

#[tokio::test]
async fn roll_back_knowledge_entry_keeps_current_name() {
    let (server, owner) = setup().await;
    let first = seed(&server, &owner, "Old").await;
    change(
        &server,
        &owner,
        &by_key("KN-1"),
        json!({"summary":"Changed"}),
    )
    .await;
    let page = list_page(
        &server,
        &owner,
        &resource(&by_key("KN-1"), "versions"),
        &[("page_size", "1")],
    )
    .await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    change(&server, &owner, &by_key("KN-1"), json!({"name":"docs/New"})).await;
    let next = list_page(
        &server,
        &owner,
        &resource(&by_name("docs/New"), "versions"),
        &[("cursor", cursor)],
    )
    .await;
    assert_eq!(next["data"], json!([version_metadata(&first)]));
    let rolled = roll_back(&server, &owner, &by_key("KN-1"), 1).await;
    assert_eq!(rolled["name"], "docs/New");
    assert_eq!(rolled["key"], first["key"]);
    assert_eq!(rolled["version"], 3);
    assert_eq!(rolled["summary"], first["summary"]);
    assert_eq!(read(&server, &owner, &by_name("docs/new")).await, rolled);
    assert_eq!(
        read(
            &server,
            &owner,
            &resource(&by_name("docs/new"), "versions/1")
        )
        .await["name"],
        "docs/New"
    );
    assert_problem(
        request(&server, &owner, Method::GET, &by_name("Old"), Value::Null).await,
        StatusCode::NOT_FOUND,
        "knowledge.not_found",
    )
    .await;
}

#[tokio::test]
async fn roll_back_knowledge_entry_fails_when_version_is_deletion() {
    let (server, owner) = setup().await;
    let first = seed(&server, &owner, "Guide").await;
    let deleted = delete(&server, &owner, &by_key("KN-1")).await;
    let path = resource(&by_key("KN-1"), "rollback");
    for body in [
        json!({"version":2}),
        json!({"version":999}),
        json!({"version":0}),
        json!({"version":-1}),
        json!({}),
        json!({"version":"1"}),
    ] {
        assert_problem(
            request(&server, &owner, Method::POST, &path, body).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, deleted);
    assert_eq!(
        list(&server, &owner, &resource(&by_key("KN-1"), "versions")).await,
        json!([version_metadata(&deleted), version_metadata(&first)])
    );
}

#[tokio::test]
async fn roll_back_knowledge_entry_fails_when_caller_is_contributor() {
    let server = two_people().await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared").await;
    participant(&server, &owner, Method::POST, "contributor").await;
    let bob = login(&server, "bob").await;
    let first = seed(&server, &owner, "Guide").await;
    for role in ["contributor", "viewer"] {
        participant(&server, &owner, Method::PATCH, role).await;
        assert_eq!(
            read(&server, &bob, &resource(&by_name("Guide"), "versions/1")).await,
            first
        );
        assert_problem(
            request(
                &server,
                &bob,
                Method::POST,
                &resource(&by_name("Guide"), "rollback"),
                json!({"version":1}),
            )
            .await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
    }
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, first);
    assert_eq!(
        list(&server, &owner, &resource(&by_key("KN-1"), "versions")).await,
        json!([version_metadata(&first)])
    );
}

#[tokio::test]
async fn create_knowledge_entry_returns_key_and_first_version() {
    let (server, owner) = setup().await;
    let created = seed(&server, &owner, "deploy/Guide.md").await;
    assert_eq!(created["key"], "KN-1");
    assert_eq!(created["version"], 1);
    assert_eq!(created["kind"], "content");
    assert_eq!(created["name"], "deploy/Guide.md");
    assert_eq!(created["summary"], "Summary");
    assert_eq!(created["content"], "Content");
    assert_eq!(created["applied_by"], "alice");
    assert!(created["applied_at"].as_i64().unwrap() > 0);
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, created);
    assert_eq!(seed(&server, &owner, "Next").await["key"], "KN-2");
}

#[tokio::test]
async fn create_knowledge_entry_fails_when_caller_is_contributor() {
    let server = two_people().await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared").await;
    let bob = login(&server, "bob").await;
    for role in ["contributor", "viewer"] {
        if role == "contributor" {
            participant(&server, &owner, Method::POST, role).await;
        } else {
            participant(&server, &owner, Method::PATCH, role).await;
        }
        assert_problem(
            request(
                &server,
                &bob,
                Method::POST,
                PATH,
                json!({"name":"Guide", "summary":"Summary", "content":"Content"}),
            )
            .await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
    }
    assert_eq!(list(&server, &owner, PATH).await, json!([]));
    participant(&server, &owner, Method::PATCH, "admin").await;
    assert_eq!(seed(&server, &bob, "Guide").await["applied_by"], "bob");
}

#[tokio::test]
async fn create_knowledge_entry_fails_when_caller_is_agent() {
    let (server, owner) = setup().await;
    let agent = authorize_for(&server, &owner, "Writer").await;
    assert_problem(
        request(
            &server,
            &agent,
            Method::POST,
            PATH,
            json!({"name":"Guide", "summary":"Summary", "content":"Content"}),
        )
        .await,
        StatusCode::FORBIDDEN,
        "auth.person_required",
    )
    .await;
    seed(&server, &owner, "Guide").await;
    for method in [Method::PATCH, Method::DELETE] {
        assert_problem(
            request(
                &server,
                &agent,
                method,
                &by_key("KN-1"),
                json!({"summary":"Other"}),
            )
            .await,
            StatusCode::FORBIDDEN,
            "auth.person_required",
        )
        .await;
    }
    assert_eq!(read(&server, &agent, &by_key("KN-1")).await["version"], 1);
}

#[tokio::test]
async fn create_knowledge_entry_fails_when_name_differs_only_in_case() {
    let (server, owner) = setup().await;
    let first = seed(&server, &owner, "Deploy/Guide.md").await;
    assert_problem(
        request(
            &server,
            &owner,
            Method::POST,
            PATH,
            json!({"name":"deploy/guide.MD", "summary":"Other", "content":"Other"}),
        )
        .await,
        StatusCode::CONFLICT,
        "knowledge.name_taken",
    )
    .await;
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, first);
}

#[tokio::test]
async fn create_knowledge_entry_fails_when_name_belongs_to_deleted_entry() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    let deleted = delete(&server, &owner, &by_key("KN-1")).await;
    let error = assert_problem(
        request(
            &server,
            &owner,
            Method::POST,
            PATH,
            json!({"name":"guide", "summary":"Other", "content":"Other"}),
        )
        .await,
        StatusCode::CONFLICT,
        "knowledge.name_taken",
    )
    .await;
    assert!(error["title"].as_str().unwrap().contains("roll it back"));
    assert_eq!(read(&server, &owner, &by_name("Guide")).await, deleted);
}

#[tokio::test]
async fn create_knowledge_entry_fails_when_name_breaks_naming_rules() {
    let (server, owner) = setup().await;
    for name in [
        "a//b",
        "/a",
        "a/",
        ".",
        "..",
        "a/./b",
        "a/../b",
        "a\u{0007}b",
        " a",
        "a ",
        "a/ b",
        "a/b ",
        "",
    ] {
        assert_problem(
            request(
                &server,
                &owner,
                Method::POST,
                PATH,
                json!({"name":name, "summary":"Summary", "content":"Content"}),
            )
            .await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    for body in [
        json!({"name":"a".repeat(201),"summary":"Summary","content":"Content"}),
        json!({"name":"Guide","summary":"s".repeat(501),"content":"Content"}),
        json!({"name":"Guide","summary":"","content":"Content"}),
        json!({"name":"Guide","summary":"Summary","content":""}),
    ] {
        assert_problem(
            request(&server, &owner, Method::POST, PATH, body).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    assert_eq!(list(&server, &owner, PATH).await, json!([]));
}

#[tokio::test]
async fn create_knowledge_entry_accepts_content_at_size_limit() {
    let (server, owner) = setup().await;
    let body = format!(
        "{{\"name\":\"{}\",\"summary\":\"{}\",\"content\":\"{}\"}}",
        "\\ud83d\\ude80".repeat(200),
        "\\ud83d\\ude80".repeat(500),
        "\\u0061".repeat(CONTENT_LIMIT)
    );
    let created = data(
        raw(
            &server,
            &owner,
            Method::POST,
            PATH,
            padded_body(body, ENTRY_BODY_LIMIT),
        )
        .await,
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(created["name"], "\u{1f680}".repeat(200));
    assert_eq!(created["summary"], "\u{1f680}".repeat(500));
    assert_eq!(created["content"], "a".repeat(CONTENT_LIMIT));
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, created);
    let body = format!("{{\"content\":\"{}\"}}", "\\u0062".repeat(CONTENT_LIMIT));
    let changed = data(
        raw(&server, &owner, Method::PATCH, &by_key("KN-1"), body).await,
        StatusCode::OK,
    )
    .await;
    assert_eq!(changed["content"], "b".repeat(CONTENT_LIMIT));
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, changed);
}

#[tokio::test]
async fn create_knowledge_entry_fails_when_content_exceeds_size_limit() {
    let (server, owner) = setup().await;
    for content in [
        "a".repeat(CONTENT_LIMIT + 1),
        "\u{00e9}".repeat(CONTENT_LIMIT / 2 + 1),
    ] {
        assert_problem(
            request(
                &server,
                &owner,
                Method::POST,
                PATH,
                json!({"name":"Guide","summary":"Summary","content":content}),
            )
            .await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    seed(&server, &owner, "Guide").await;
    for (method, path) in [
        (Method::POST, PATH.to_owned()),
        (Method::PATCH, by_key("KN-1")),
        (Method::PATCH, by_name("Guide")),
    ] {
        let body = padded_body(
            json!({"name":"Guide","summary":"Summary","content":"a".repeat(CONTENT_LIMIT)})
                .to_string(),
            ENTRY_BODY_LIMIT + 1,
        );
        assert_problem(
            raw(&server, &owner, method, &path, body).await,
            StatusCode::PAYLOAD_TOO_LARGE,
            "request.body_too_large",
        )
        .await;
    }
    let body = padded_body(
        json!({"name":"Space","summary":"Summary"}).to_string(),
        2 * CONTENT_LIMIT + 1,
    );
    assert_problem(
        raw(&server, &owner, Method::POST, "/api/v1/spaces", body).await,
        StatusCode::PAYLOAD_TOO_LARGE,
        "request.body_too_large",
    )
    .await;
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await["version"], 1);
}

#[tokio::test]
async fn change_knowledge_entry_adds_version_that_readers_see() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    let agent = authorize_for(&server, &owner, "Reader").await;
    let changed = change(
        &server,
        &owner,
        &by_name("guide"),
        json!({"content":"New content", "summary":"New summary"}),
    )
    .await;
    assert_eq!(changed["version"], 2);
    assert_eq!(changed["content"], "New content");
    assert_eq!(changed["summary"], "New summary");
    assert_eq!(changed["applied_by"], "alice");
    assert_eq!(read(&server, &agent, &by_key("KN-1")).await, changed);
}

#[tokio::test]
async fn change_knowledge_entry_keeps_field_left_out() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    let first = change(
        &server,
        &owner,
        &by_key("KN-1"),
        json!({"summary":"New summary"}),
    )
    .await;
    assert_eq!(first["content"], "Content");
    let second = change(
        &server,
        &owner,
        &by_name("Guide"),
        json!({"content":"New content"}),
    )
    .await;
    assert_eq!(second["summary"], "New summary");
    assert_eq!(second["version"], 3);
    for body in [
        json!({}),
        json!({"summary":null}),
        json!({"content":""}),
        json!({"name":"a/../b"}),
    ] {
        assert_problem(
            request(&server, &owner, Method::PATCH, &by_key("KN-1"), body).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, second);
}

#[tokio::test]
async fn rename_knowledge_entry_keeps_key_and_adds_no_version() {
    let (server, owner) = setup().await;
    let first = seed(&server, &owner, "Guide").await;
    let changed = change(
        &server,
        &owner,
        &by_key("KN-1"),
        json!({"name":"docs/Guide.md"}),
    )
    .await;
    assert_eq!(changed["key"], first["key"]);
    assert_eq!(changed["version"], 1);
    assert_eq!(changed["content"], first["content"]);
    assert_eq!(changed["applied_at"], first["applied_at"]);
    assert_eq!(
        read(&server, &owner, &by_name("docs/Guide.md")).await,
        changed
    );
    assert_problem(
        request(&server, &owner, Method::GET, &by_name("Guide"), Value::Null).await,
        StatusCode::NOT_FOUND,
        "knowledge.not_found",
    )
    .await;
}

#[tokio::test]
async fn rename_knowledge_entry_succeeds_when_only_case_changes() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    let changed = change(&server, &owner, &by_name("GUIDE"), json!({"name":"GUIDE"})).await;
    assert_eq!(changed["name"], "GUIDE");
    assert_eq!(changed["key"], "KN-1");
    assert_eq!(changed["version"], 1);
    assert_eq!(read(&server, &owner, &by_name("guide")).await, changed);
}

#[tokio::test]
async fn rename_knowledge_entry_fails_when_name_belongs_to_another_entry() {
    let (server, owner) = setup().await;
    let original = seed(&server, &owner, "Guide").await;
    seed(&server, &owner, "Other").await;
    for deleted in [false, true] {
        if deleted {
            delete(&server, &owner, &by_key("KN-2")).await;
        }
        let error = assert_problem(
            request(
                &server,
                &owner,
                Method::PATCH,
                &by_key("KN-1"),
                json!({"name":"OTHER", "content":"Must not apply"}),
            )
            .await,
            StatusCode::CONFLICT,
            "knowledge.name_taken",
        )
        .await;
        if deleted {
            assert!(error["title"].as_str().unwrap().contains("roll it back"));
        }
        assert_eq!(read(&server, &owner, &by_key("KN-1")).await, original);
    }
}

#[tokio::test]
async fn delete_knowledge_entry_hides_entry_from_agents() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    let agent = authorize_for(&server, &owner, "Reader").await;
    assert_eq!(
        read(&server, &agent, &by_key("KN-1")).await["content"],
        "Content"
    );
    let deleted = delete(&server, &owner, &by_name("Guide")).await;
    assert_eq!(deleted["kind"], "deletion");
    assert_eq!(deleted["version"], 2);
    assert_problem(
        request(&server, &agent, Method::GET, &by_key("KN-1"), Value::Null).await,
        StatusCode::NOT_FOUND,
        "knowledge.not_found",
    )
    .await;
    for method in [Method::PATCH, Method::DELETE] {
        assert_problem(
            request(
                &server,
                &owner,
                method,
                &by_key("KN-1"),
                json!({"content":"No"}),
            )
            .await,
            StatusCode::NOT_FOUND,
            "knowledge.not_found",
        )
        .await;
    }
    assert_eq!(list(&server, &agent, PATH).await, json!([]));
    assert_eq!(list(&server, &owner, PATH).await, json!([]));
}

#[tokio::test]
async fn read_knowledge_entry_returns_deletion_when_person_reads_deleted_entry() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "Guide").await;
    let deleted = delete(&server, &owner, &by_key("KN-1")).await;
    assert_eq!(deleted["applied_by"], "alice");
    assert!(deleted["applied_at"].as_i64().unwrap() > 0);
    assert_eq!(deleted["summary"], Value::Null);
    assert_eq!(deleted["content"], Value::Null);
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, deleted);
}

#[tokio::test]
async fn read_knowledge_entry_finds_entry_when_name_case_differs() {
    let (server, owner) = setup().await;
    let entry = seed(&server, &owner, "Deploy/Guide.md").await;
    assert_eq!(
        read(&server, &owner, &by_name("DEPLOY/guide.MD")).await,
        entry
    );
}

#[tokio::test]
async fn read_knowledge_entry_finds_entry_by_key() {
    let (server, owner) = setup().await;
    let first = seed(&server, &owner, "Guide").await;
    let key_named = seed(&server, &owner, "KN-1").await;
    assert_eq!(read(&server, &owner, &by_key("KN-1")).await, first);
    assert_eq!(read(&server, &owner, &by_key("KN-2")).await, key_named);
    assert_eq!(read(&server, &owner, &by_name("KN-1")).await, key_named);
    for path in [
        format!("{PATH}/entry"),
        format!("{}?name=Guide", by_key("KN-1")),
        format!("{PATH}/entry?name=Guide&key=KN-1"),
    ] {
        assert_problem(
            request(&server, &owner, Method::GET, &path, Value::Null).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    assert_problem(
        request(&server, &owner, Method::GET, &by_key("KN-99"), Value::Null).await,
        StatusCode::NOT_FOUND,
        "knowledge.not_found",
    )
    .await;
}

#[tokio::test]
async fn read_knowledge_entry_fails_when_caller_is_not_in_space() {
    let server = two_people().await;
    let owner = server.tokens().await;
    create_space(&server, &owner, "Team", "Shared").await;
    seed(&server, &owner, "Guide").await;
    let bob = login(&server, "bob").await;
    for method in [Method::GET, Method::PATCH, Method::DELETE] {
        assert_problem(
            request(
                &server,
                &bob,
                method,
                &by_key("KN-1"),
                json!({"content":"Denied"}),
            )
            .await,
            StatusCode::NOT_FOUND,
            "space.not_found",
        )
        .await;
    }
    assert_problem(
        list_response(PATH, &server, &bob, &[]).await,
        StatusCode::NOT_FOUND,
        "space.not_found",
    )
    .await;
    participant(&server, &owner, Method::POST, "viewer").await;
    let viewer = authorize_for(&server, &bob, "Viewer").await;
    assert_eq!(
        read(&server, &viewer, &by_key("KN-1")).await["content"],
        "Content"
    );
    assert_eq!(list(&server, &bob, PATH).await[0]["key"], "KN-1");
}

#[tokio::test]
async fn list_knowledge_returns_entries_and_next_level_prefixes() {
    let (server, owner) = setup().await;
    for name in [
        "a.md",
        "deploy/one.md",
        "deploy/deep/two.md",
        "deploy-old/other.md",
        "z.md",
    ] {
        seed(&server, &owner, name).await;
    }
    let root = list(&server, &owner, PATH).await;
    assert_eq!(
        root,
        json!([{"name":"a.md","key":"KN-1","summary":"Summary"},{"name":"deploy-old/"},{"name":"deploy/"},{"name":"z.md","key":"KN-5","summary":"Summary"}])
    );
    let prefix = root[2]["name"].as_str().unwrap();
    let level = list_page(&server, &owner, PATH, &[("prefix", prefix)]).await;
    assert_eq!(
        level["data"],
        json!([{"name":"deploy/deep/"},{"name":"deploy/one.md","key":"KN-2","summary":"Summary"}])
    );
    assert_eq!(
        list_page(&server, &owner, PATH, &[("prefix", "DEPLOY")]).await,
        level
    );
    let deeper = list_page(
        &server,
        &owner,
        PATH,
        &[("prefix", level["data"][0]["name"].as_str().unwrap())],
    )
    .await;
    assert_eq!(deeper["data"][0]["key"], "KN-3");
    delete(&server, &owner, &by_key("KN-3")).await;
    assert_eq!(
        list_page(&server, &owner, PATH, &[("prefix", prefix)]).await["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    for prefix in ["\u{77e5}\u{8bc6}", "\u{0130}"] {
        unicode_level(&server, &owner, prefix).await;
    }
}

#[tokio::test]
async fn list_knowledge_merges_levels_that_differ_only_in_case() {
    let (server, owner) = setup().await;
    for name in ["Deploy/Sub/one", "deploy/sub/two", "DEPLOY/Other/three"] {
        seed(&server, &owner, name).await;
    }
    assert_eq!(
        list(&server, &owner, PATH).await,
        json!([{"name":"Deploy/"}])
    );
    assert_eq!(
        list_page(&server, &owner, PATH, &[("prefix", "deploy/")]).await["data"],
        json!([{"name":"DEPLOY/Other/"},{"name":"Deploy/Sub/"}])
    );
    delete(&server, &owner, &by_key("KN-1")).await;
    assert_eq!(
        list(&server, &owner, PATH).await,
        json!([{"name":"deploy/"}])
    );
}

#[tokio::test]
async fn list_knowledge_follows_cursor_through_level() {
    let (server, owner) = setup().await;
    for name in ["D", "b/first", "b/second", "C", "a"] {
        seed(&server, &owner, name).await;
    }
    let rows = walk(&server, &owner, PATH, "1", 4).await;
    assert_eq!(
        rows.iter()
            .map(|row| row["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["a", "b/", "C", "D"]
    );
    let page = list_page(&server, &owner, PATH, &[("page_size", "1")]).await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    for extra in [("prefix", "b/"), ("keyword", "first")] {
        assert_problem(
            list_response(PATH, &server, &owner, &[("cursor", cursor), extra]).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
    let page = list_page(
        &server,
        &owner,
        PATH,
        &[("prefix", "B/"), ("page_size", "1")],
    )
    .await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    let next = list_page(
        &server,
        &owner,
        PATH,
        &[("prefix", "b"), ("cursor", cursor)],
    )
    .await;
    assert_eq!(next["data"][0]["key"], "KN-3");
    let page = list_page(
        &server,
        &owner,
        PATH,
        &[("keyword", "s"), ("page_size", "1")],
    )
    .await;
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    assert_problem(
        list_response(
            PATH,
            &server,
            &owner,
            &[("keyword", "first"), ("cursor", cursor)],
        )
        .await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
}

#[tokio::test]
async fn list_knowledge_matches_keyword_in_name_summary_and_content() {
    let (server, owner) = setup().await;
    create(&server, &owner, "deploy/needle", "Other", "Other").await;
    create(&server, &owner, "Summary", "needle", "Other").await;
    create(
        &server,
        &owner,
        "Content",
        "Other",
        "needle \u{77e5}\u{8bc6}",
    )
    .await;
    seed(&server, &owner, "Unrelated").await;
    let matches = keyword_pages(&server, &owner, "NEEDLE", 3).await;
    assert_eq!(matches[1]["name"], "deploy/needle");
    let chinese = list_page(&server, &owner, PATH, &[("keyword", "\u{77e5}\u{8bc6}")]).await;
    assert_eq!(chinese["data"][0]["key"], "KN-3");
    assert_eq!(chinese["data"].as_array().unwrap().len(), 1);
    for keyword in ["".to_owned(), " ".to_owned(), "a".repeat(201)] {
        assert_problem(
            list_response(PATH, &server, &owner, &[("keyword", &keyword)]).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
}

#[tokio::test]
async fn list_knowledge_matches_keyword_literally_when_it_holds_wildcards() {
    let (server, owner) = setup().await;
    create(&server, &owner, "literal", "A%_B\\", "Exact phrase").await;
    create(&server, &owner, "wildcards", "AxxB/", "Exact other phrase").await;
    for keyword in ["%", "_", "\\", "a%_b\\", "Exact phrase"] {
        let page = list_page(&server, &owner, PATH, &[("keyword", keyword)]).await;
        assert_eq!(page["data"].as_array().unwrap().len(), 1);
        assert_eq!(page["data"][0]["key"], "KN-1");
    }
}

#[tokio::test]
async fn list_knowledge_ignores_old_versions_and_deleted_entries_when_searching() {
    let (server, owner) = setup().await;
    create(&server, &owner, "first", "needle", "needle").await;
    create(&server, &owner, "second", "needle", "needle").await;
    change(
        &server,
        &owner,
        &by_key("KN-1"),
        json!({"content":"New", "summary":"New"}),
    )
    .await;
    delete(&server, &owner, &by_key("KN-2")).await;
    assert_eq!(
        list_page(&server, &owner, PATH, &[("keyword", "needle")]).await["data"],
        json!([])
    );
    assert_eq!(
        list_page(&server, &owner, PATH, &[("keyword", "new")]).await["data"][0]["key"],
        "KN-1"
    );
}

#[tokio::test]
async fn list_knowledge_fails_when_prefix_and_keyword_are_both_given() {
    let (server, owner) = setup().await;
    seed(&server, &owner, "deploy/Guide").await;
    assert_problem(
        list_response(
            PATH,
            &server,
            &owner,
            &[("prefix", "deploy"), ("keyword", "Guide")],
        )
        .await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
    for prefix in ["deploy//", "/", "../deploy"] {
        assert_problem(
            list_response(PATH, &server, &owner, &[("prefix", prefix)]).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
    assert_eq!(
        list(&server, &owner, PATH).await,
        json!([{"name":"deploy/"}])
    );
}
