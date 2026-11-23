use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use super::helpers::{
    Server, assert_problem, authorize_for, create_space, list, list_response, login, request, walk,
};
use helpers::*;

mod helpers;

#[tokio::test]
async fn add_space_member_fails_when_caller_is_agent() {
    let (server, owner) = setup().await;
    let agent = authorize_for(&server, &owner, "Owner agent").await;
    let before = list(&server, &owner, SPACE_MEMBERS).await;
    assert_problem(
        mutation(&server, &agent, Method::POST, "bob", "contributor").await,
        StatusCode::FORBIDDEN,
        "auth.person_required",
    )
    .await;
    assert_eq!(list(&server, &owner, SPACE_MEMBERS).await, before);
    server.stop().await;
}

#[tokio::test]
async fn add_space_member_lets_person_and_their_agent_enter_space() {
    let (server, owner) = setup().await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &bob, "Bob agent").await;
    for tokens in [&bob, &agent] {
        assert_problem(
            request(&server, tokens, Method::GET, SPACE, Value::Null).await,
            StatusCode::NOT_FOUND,
            "space.not_found",
        )
        .await;
    }
    add(&server, &owner, "bob", "contributor").await;
    for role in ["contributor", "viewer", "contributor"] {
        change(&server, &owner, "bob", role).await;
        for tokens in [&bob, &agent] {
            assert_role(&server, tokens, role).await;
            assert_eq!(
                list(&server, tokens, SPACE_MEMBERS).await,
                list(&server, &owner, SPACE_MEMBERS).await
            );
        }
    }
    server.stop().await;
}

#[tokio::test]
async fn add_space_member_fails_when_caller_is_contributor() {
    let (server, owner) = setup().await;
    let bob = login(&server, "bob").await;
    add(&server, &owner, "bob", "contributor").await;
    add(&server, &owner, "carol", "viewer").await;
    for role in ["contributor", "viewer"] {
        change(&server, &owner, "bob", role).await;
        for method in [Method::POST, Method::PATCH, Method::DELETE] {
            assert_problem(
                mutation(&server, &bob, method, "carol", "contributor").await,
                StatusCode::FORBIDDEN,
                "space.permission_required",
            )
            .await;
        }
        assert_target_role(&server, &owner, "bob", role).await;
        assert_target_role(&server, &owner, "carol", "viewer").await;
    }
    server.stop().await;
}

#[tokio::test]
async fn add_space_member_fails_when_role_is_admin() {
    let (server, owner) = setup().await;
    for role in ["admin", "owner", "invalid", ""] {
        assert_problem(
            mutation(&server, &owner, Method::POST, "bob", role).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_body",
        )
        .await;
    }
    assert_eq!(
        list(&server, &owner, SPACE_MEMBERS).await,
        json!([{"type":"human", "username":"alice", "role":"owner"}])
    );
    server.stop().await;
}

#[tokio::test]
async fn add_space_member_fails_when_username_is_not_in_users_file() {
    let (server, owner) = setup().await;
    assert_problem(
        mutation(&server, &owner, Method::POST, "unknown", "contributor").await,
        StatusCode::NOT_FOUND,
        "user.not_found",
    )
    .await;
    add(&server, &owner, "bob", "viewer").await;
    assert_problem(
        mutation(&server, &owner, Method::POST, "bob", "contributor").await,
        StatusCode::CONFLICT,
        "space_member.already_exists",
    )
    .await;
    assert_target_role(&server, &owner, "bob", "viewer").await;
    for username in ["carol", "unknown"] {
        for method in [Method::PATCH, Method::DELETE] {
            assert_problem(
                mutation(&server, &owner, method, username, "contributor").await,
                StatusCode::NOT_FOUND,
                "space_member.not_found",
            )
            .await;
        }
    }
    let carol = login(&server, "carol").await;
    assert_hidden_space(&server, &carol).await;
    for key in ["SPACE-1", "SPACE-999"] {
        for method in [Method::POST, Method::PATCH, Method::DELETE] {
            let response = request(
                &server,
                &carol,
                method,
                &format!("/api/v1/spaces/{key}/members/bob"),
                json!({"role":"viewer"}),
            )
            .await;
            assert_problem(response, StatusCode::NOT_FOUND, "space.not_found").await;
        }
    }
    assert_target_role(&server, &owner, "bob", "viewer").await;
    server.stop().await;
}

#[tokio::test]
async fn change_space_member_role_fails_when_caller_is_agent() {
    let (server, owner) = setup().await;
    let agent = authorize_for(&server, &owner, "Owner agent").await;
    add(&server, &owner, "bob", "viewer").await;
    let before = list(&server, &owner, SPACE_MEMBERS).await;
    assert_problem(
        mutation(&server, &agent, Method::PATCH, "bob", "contributor").await,
        StatusCode::FORBIDDEN,
        "auth.person_required",
    )
    .await;
    assert_eq!(list(&server, &owner, SPACE_MEMBERS).await, before);
    server.stop().await;
}

#[tokio::test]
async fn change_space_member_role_makes_admin_when_caller_is_owner() {
    let (server, owner) = setup().await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &bob, "Bob agent").await;
    add(&server, &owner, "bob", "viewer").await;
    change(&server, &owner, "bob", "admin").await;
    assert_role(&server, &bob, "admin").await;
    assert_role(&server, &agent, "contributor").await;
    let response = request(
        &server,
        &bob,
        Method::PATCH,
        SPACE,
        json!({"name":"Renamed", "summary":"Admin summary"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let space = response.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(
        space,
        json!({"key":"SPACE-1", "name":"Renamed", "summary":"Admin summary", "role":"admin", "caller_person":"bob"})
    );
    assert_role(&server, &bob, "admin").await;
    assert_eq!(list(&server, &bob, "/api/v1/spaces").await, json!([space]));
    add(&server, &bob, "carol", "viewer").await;
    change(&server, &bob, "carol", "contributor").await;
    change(&server, &bob, "carol", "viewer").await;
    assert_target_role(&server, &owner, "carol", "viewer").await;
    let response = mutation(&server, &bob, Method::DELETE, "carol", "").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !list(&server, &owner, SPACE_MEMBERS)
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["username"] == "carol")
    );
    change(&server, &owner, "bob", "viewer").await;
    assert_role(&server, &agent, "viewer").await;
    assert_problem(
        request(
            &server,
            &bob,
            Method::PATCH,
            SPACE,
            json!({"name":"Denied"}),
        )
        .await,
        StatusCode::FORBIDDEN,
        "space.permission_required",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn change_space_member_role_fails_when_admin_changes_another_admin() {
    let (server, owner) = setup().await;
    let bob = login(&server, "bob").await;
    for username in ["bob", "carol", "dave"] {
        add(&server, &owner, username, "contributor").await;
    }
    for username in ["bob", "carol"] {
        change(&server, &owner, username, "admin").await;
    }
    for (username, role) in [
        ("carol", "viewer"),
        ("bob", "contributor"),
        ("dave", "admin"),
        ("dave", "owner"),
        ("alice", "viewer"),
    ] {
        assert_problem(
            mutation(&server, &bob, Method::PATCH, username, role).await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
    }
    for (username, role) in [("alice", "admin"), ("bob", "owner")] {
        assert_problem(
            mutation(&server, &owner, Method::PATCH, username, role).await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
    }
    assert_target_role(&server, &owner, "alice", "owner").await;
    assert_target_role(&server, &owner, "bob", "admin").await;
    assert_target_role(&server, &owner, "carol", "admin").await;
    assert_target_role(&server, &owner, "dave", "contributor").await;
    change(&server, &owner, "carol", "viewer").await;
    assert_target_role(&server, &owner, "carol", "viewer").await;
    server.stop().await;
}

#[tokio::test]
async fn change_space_fails_when_caller_is_contributor() {
    let (server, owner) = setup().await;
    let bob = login(&server, "bob").await;
    add(&server, &owner, "bob", "contributor").await;
    for role in ["contributor", "viewer"] {
        change(&server, &owner, "bob", role).await;
        assert_problem(
            request(
                &server,
                &bob,
                Method::PATCH,
                SPACE,
                json!({"name":"Denied", "summary":"Denied"}),
            )
            .await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
        let response = request(&server, &owner, Method::GET, SPACE, Value::Null).await;
        assert_eq!(
            response.json::<Value>().await.unwrap()["data"],
            json!({"key":"SPACE-1", "name":"Team", "summary":"Shared work", "role":"owner", "caller_person":"alice"})
        );
    }
    server.stop().await;
}

#[tokio::test]
async fn remove_space_member_blocks_person_and_their_agent() {
    let (server, owner) = setup().await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &bob, "Bob agent").await;
    add(&server, &owner, "bob", "contributor").await;
    for tokens in [&bob, &agent] {
        assert_role(&server, tokens, "contributor").await;
    }
    assert_eq!(
        list(&server, &owner, SPACE_MEMBERS)
            .await
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let response = mutation(&server, &owner, Method::DELETE, "bob", "").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data":null,"meta":{}})
    );
    for tokens in [&bob, &agent] {
        assert_hidden_space(&server, tokens).await;
        assert_eq!(list(&server, tokens, "/api/v1/spaces").await, json!([]));
    }
    assert_eq!(
        list(&server, &owner, SPACE_MEMBERS).await,
        json!([{"type":"human", "username":"alice", "role":"owner"}])
    );
    add(&server, &owner, "bob", "viewer").await;
    for tokens in [&bob, &agent] {
        assert_role(&server, tokens, "viewer").await;
    }
    server.stop().await;
}

#[tokio::test]
async fn remove_space_member_fails_when_caller_is_agent() {
    let (server, owner) = setup().await;
    let agent = authorize_for(&server, &owner, "Owner agent").await;
    add(&server, &owner, "bob", "contributor").await;
    let before = list(&server, &owner, SPACE_MEMBERS).await;
    assert_problem(
        mutation(&server, &agent, Method::DELETE, "bob", "").await,
        StatusCode::FORBIDDEN,
        "auth.person_required",
    )
    .await;
    assert_eq!(list(&server, &owner, SPACE_MEMBERS).await, before);
    server.stop().await;
}

#[tokio::test]
async fn remove_space_member_fails_when_target_is_owner() {
    let (server, owner) = setup().await;
    assert_problem(
        mutation(&server, &owner, Method::DELETE, "alice", "").await,
        StatusCode::FORBIDDEN,
        "space.permission_required",
    )
    .await;
    assert_role(&server, &owner, "owner").await;
    assert_target_role(&server, &owner, "alice", "owner").await;
    server.stop().await;
}

#[tokio::test]
async fn remove_space_member_fails_when_admin_removes_admin() {
    let (server, owner) = setup().await;
    let bob = login(&server, "bob").await;
    for username in ["bob", "carol"] {
        add(&server, &owner, username, "contributor").await;
        change(&server, &owner, username, "admin").await;
    }
    for username in ["bob", "carol", "alice"] {
        assert_problem(
            mutation(&server, &bob, Method::DELETE, username, "").await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
    }
    assert_target_role(&server, &owner, "bob", "admin").await;
    assert_target_role(&server, &owner, "carol", "admin").await;
    assert_target_role(&server, &owner, "alice", "owner").await;
    let response = mutation(&server, &owner, Method::DELETE, "carol", "").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_hidden_space(&server, &login(&server, "carol").await).await;
    server.stop().await;
}

#[tokio::test]
async fn list_space_members_includes_agents_with_capped_roles() {
    let (server, owner) = setup().await;
    let agent = authorize_for(&server, &owner, "Shared agent").await;
    authorize_for(&server, &owner, "Second agent").await;
    let mut expected = vec![
        json!({"type":"human","username":"alice","role":"owner"}),
        json!({"type":"agent","username":"alice","agent_name":"Second agent","role":"contributor"}),
        json!({"type":"agent","username":"alice","agent_name":"Shared agent","role":"contributor"}),
    ];
    for (username, role) in [
        ("bob", "admin"),
        ("carol", "contributor"),
        ("dave", "viewer"),
    ] {
        add(&server, &owner, username, "contributor").await;
        change(&server, &owner, username, role).await;
        let tokens = login(&server, username).await;
        authorize_for(&server, &tokens, "Shared agent").await;
        expected.push(json!({"type":"human","username":username,"role":role}));
        expected.push(json!({"type":"agent","username":username,"agent_name":"Shared agent","role":if role == "viewer" {"viewer"} else {"contributor"}}));
    }
    for size in ["1", "2", "4"] {
        assert_eq!(
            walk(&server, &agent, SPACE_MEMBERS, size, 99).await,
            expected
        );
    }
    assert_eq!(list(&server, &owner, SPACE_MEMBERS).await, json!(expected));
    assert_roster_cursors(&server, &owner, &agent).await;
    let response = request(
        &server,
        &owner,
        Method::DELETE,
        "/api/v1/agents/Second%20agent",
        Value::Null,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    expected.remove(1);
    assert_eq!(
        walk(&server, &agent, SPACE_MEMBERS, "2", 99).await,
        expected
    );
    server.stop().await;
}

#[tokio::test]
async fn list_space_members_omits_person_when_account_was_removed_from_users_file() {
    let (server, owner) = setup().await;
    let bob = login(&server, "bob").await;
    let agent = authorize_for(&server, &bob, "Bob agent").await;
    add(&server, &owner, "bob", "contributor").await;
    create_space(&server, &owner, "Team", "Shared work").await;
    let response = request(
        &server,
        &owner,
        Method::POST,
        "/api/v1/spaces/SPACE-2/members/bob",
        json!({"role":"viewer"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let directory = server.stop().await;
    let server = Server::restart_with_users(directory, &USERS[..1]).await;
    let owner = server.tokens().await;
    for key in ["SPACE-1", "SPACE-2"] {
        let path = format!("/api/v1/spaces/{key}/members");
        assert_eq!(
            list(&server, &owner, &path).await,
            json!([{"type":"human","username":"alice","role":"owner"}])
        );
    }
    for tokens in [&bob, &agent] {
        assert_problem(
            list_response(SPACE_MEMBERS, &server, tokens, &[]).await,
            StatusCode::UNAUTHORIZED,
            "auth.unauthorized",
        )
        .await;
    }
    assert_problem(
        mutation(&server, &owner, Method::POST, "bob", "viewer").await,
        StatusCode::NOT_FOUND,
        "user.not_found",
    )
    .await;
    let directory = server.stop().await;
    let server = Server::restart_with_users(directory, USERS).await;
    let bob = login(&server, "bob").await;
    assert_eq!(list(&server, &bob, "/api/v1/spaces").await, json!([]));
    assert_eq!(list(&server, &bob, "/api/v1/agents").await, json!([]));
    let owner = server.tokens().await;
    assert_target_role(&server, &owner, "alice", "owner").await;
    assert_hidden_space(&server, &bob).await;
    server.stop().await;
}

async fn assert_hidden_space(server: &Server, tokens: &Value) {
    let mut expected = None;
    for key in ["SPACE-1", "SPACE-999"] {
        for suffix in ["", "/members"] {
            let path = format!("/api/v1/spaces/{key}{suffix}");
            let problem = assert_problem(
                request(server, tokens, Method::GET, &path, Value::Null).await,
                StatusCode::NOT_FOUND,
                "space.not_found",
            )
            .await;
            if let Some(expected) = &expected {
                assert_eq!(&problem, expected);
            } else {
                expected = Some(problem);
            }
        }
    }
}

async fn assert_roster_cursors(server: &Server, owner: &Value, agent: &Value) {
    let response = list_response(SPACE_MEMBERS, server, agent, &[("page_size", "1")]).await;
    let page = response.json::<Value>().await.unwrap();
    let cursor = page["meta"]["next_cursor"].as_str().unwrap();
    let bob = login(server, "bob").await;
    assert_problem(
        list_response(SPACE_MEMBERS, server, &bob, &[("cursor", cursor)]).await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
    create_space(server, owner, "Team", "Shared work").await;
    let second_roster = list(server, owner, "/api/v1/spaces/SPACE-2/members").await;
    assert_eq!(second_roster.as_array().unwrap().len(), 3);
    assert!(
        second_roster
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["username"] == "alice")
    );
    assert_problem(
        list_response(
            "/api/v1/spaces/SPACE-2/members",
            server,
            owner,
            &[("cursor", cursor)],
        )
        .await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
    assert_problem(
        list_response("/api/v1/users", server, owner, &[("cursor", cursor)]).await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
    for query in [
        [("page_size", "0")],
        [("page_size", "101")],
        [("cursor", "invalid")],
    ] {
        assert_problem(
            list_response(SPACE_MEMBERS, server, agent, &query).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
}
