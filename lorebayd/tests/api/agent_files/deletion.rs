use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, assert_problem, authorize_for, create_space, login, request};
use crate::tasks::helpers::{SPACE, TASKS, create, participant, read, setup};

use super::helpers::{complete, register, set_status, upload, upload_ok};

#[tokio::test]
async fn delete_agent_file_removes_agent_file_from_task_and_its_file() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Mistaken file").await;
    let agent_file = uploaded(&server, &agent, &task).await;
    let retained = uploaded(&server, &agent, &task).await;
    assert_eq!(
        read(&server, &person, "TK-1").await["files"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_exists(&server, &agent_file).await;
    assert_problem(
        remove(&server, &person, SPACE, &json!({"task_id":"TK-1"}), "FL-1").await,
        StatusCode::NOT_FOUND,
        "task.not_found",
    )
    .await;
    let before = set_status(&server, &person, &task, "ended").await;
    assert_deleted(remove(&server, &person, SPACE, &task, "FL-1").await).await;
    let mut expected = before;
    expected["files"].as_array_mut().unwrap().remove(0);
    assert_eq!(read(&server, &person, "TK-1").await, expected);
    upload_ok(&server, &agent_file, "Removed file location").await;
    assert_exists(&server, &retained).await;
    assert_problem(
        remove(&server, &person, SPACE, &task, "FL-1").await,
        StatusCode::NOT_FOUND,
        "agent_file.not_found",
    )
    .await;
    assert_eq!(read(&server, &person, "TK-1").await, expected);
    server.stop().await;
}

#[tokio::test]
async fn delete_agent_file_fails_when_caller_is_agent() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Agent cannot delete").await;
    let agent_file = uploaded(&server, &agent, &task).await;
    let before = read(&server, &person, "TK-1").await;
    for space in [SPACE, "SPACE-999"] {
        assert_problem(
            remove(&server, &agent, space, &task, "FL-1").await,
            StatusCode::FORBIDDEN,
            "auth.person_required",
        )
        .await;
    }
    assert_eq!(read(&server, &person, "TK-1").await, before);
    assert_exists(&server, &agent_file).await;
    server.stop().await;
}

#[tokio::test]
async fn delete_agent_file_fails_when_agent_file_belongs_to_another_person() {
    let (server, owner, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Alice file").await;
    let agent_file = uploaded(&server, &agent, &task).await;
    let before = read(&server, &owner, "TK-1").await;
    let bob = login(&server, "bob").await;
    participant(&server, &owner, Method::POST, "contributor").await;
    assert_problem(
        remove(&server, &bob, SPACE, &task, "FL-1").await,
        StatusCode::FORBIDDEN,
        "space.permission_required",
    )
    .await;
    assert_eq!(read(&server, &owner, "TK-1").await, before);
    let bob_agent = authorize_for(&server, &bob, "Bob file agent").await;
    let own = create(&server, &bob_agent, SPACE, "Bob file").await;
    let own_agent_file = uploaded(&server, &bob_agent, &own).await;
    assert_deleted(remove(&server, &owner, SPACE, &own, "FL-2").await).await;
    assert_eq!(read(&server, &owner, "TK-2").await["files"], json!([]));
    upload_ok(&server, &own_agent_file, "Owner removed file").await;
    participant(&server, &owner, Method::DELETE, "").await;
    create_space(&server, &bob, "Other", "Bob space").await;
    for (space, code) in [("SPACE-2", "task.not_found"), (SPACE, "space.not_found")] {
        assert_problem(
            remove(&server, &bob, space, &task, "FL-1").await,
            StatusCode::NOT_FOUND,
            code,
        )
        .await;
    }
    assert_eq!(read(&server, &owner, "TK-1").await, before);
    assert_exists(&server, &agent_file).await;
    participant(&server, &owner, Method::POST, "contributor").await;
    participant(&server, &owner, Method::PATCH, "admin").await;
    assert_deleted(remove(&server, &bob, SPACE, &task, "FL-1").await).await;
    assert_eq!(read(&server, &owner, "TK-1").await["files"], json!([]));
    upload_ok(&server, &agent_file, "Admin removed file").await;
    server.stop().await;
}

#[tokio::test]
async fn delete_agent_file_fails_when_person_became_viewer() {
    let (server, owner, _) = setup().await;
    let bob = login(&server, "bob").await;
    participant(&server, &owner, Method::POST, "contributor").await;
    let agent = authorize_for(&server, &bob, "Bob file agent").await;
    let task = create(&server, &agent, SPACE, "Earlier file").await;
    let agent_file = uploaded(&server, &agent, &task).await;
    let before = read(&server, &bob, "TK-1").await;
    participant(&server, &owner, Method::PATCH, "viewer").await;
    assert_problem(
        remove(&server, &bob, SPACE, &task, "FL-1").await,
        StatusCode::FORBIDDEN,
        "space.permission_required",
    )
    .await;
    assert_eq!(read(&server, &bob, "TK-1").await, before);
    assert_exists(&server, &agent_file).await;
    participant(&server, &owner, Method::PATCH, "contributor").await;
    assert_deleted(remove(&server, &bob, SPACE, &task, "FL-1").await).await;
    assert_eq!(read(&server, &bob, "TK-1").await["files"], json!([]));
    upload_ok(&server, &agent_file, "Contributor removed file").await;
    server.stop().await;
}

#[tokio::test]
async fn delete_agent_file_fails_when_agent_file_is_in_another_task() {
    let (server, person, agent) = setup().await;
    let first = create(&server, &agent, SPACE, "First task").await;
    let first_agent_file = uploaded(&server, &agent, &first).await;
    let second = create(&server, &agent, SPACE, "Second task").await;
    uploaded(&server, &agent, &second).await;
    let pending = register(&server, &agent, &second).await;
    upload_ok(&server, &pending, "Unreported upload").await;
    let before = read(&server, &person, "TK-2").await;
    for key in ["FL-1", "FL-3", "FL-999", "invalid", "TK-2"] {
        assert_problem(
            remove(&server, &person, SPACE, &second, key).await,
            StatusCode::NOT_FOUND,
            "agent_file.not_found",
        )
        .await;
    }
    assert_eq!(read(&server, &person, "TK-2").await, before);
    assert_exists(&server, &first_agent_file).await;
    assert_exists(&server, &pending).await;
    assert_eq!(
        complete(&server, &agent, &second, "FL-3").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        read(&server, &person, "TK-2").await["files"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    server.stop().await;
}

#[tokio::test]
async fn delete_task_removes_content_of_its_agent_files() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Task with files").await;
    let completed = uploaded(&server, &agent, &task).await;
    let pending = register(&server, &agent, &task).await;
    upload_ok(&server, &pending, "Unreported upload").await;
    register(&server, &agent, &task).await;
    let other = create(&server, &agent, SPACE, "Other task").await;
    let retained = uploaded(&server, &agent, &other).await;
    for agent_file in [&completed, &pending] {
        assert_exists(&server, agent_file).await;
    }
    assert_deleted(
        request(
            &server,
            &person,
            Method::DELETE,
            &format!("{TASKS}/{}", task["task_id"].as_str().unwrap()),
            Value::Null,
        )
        .await,
    )
    .await;
    for agent_file in [&completed, &pending] {
        upload_ok(&server, agent_file, "Deleted task file location").await;
        assert_problem(
            complete(&server, &agent, &task, agent_file["key"].as_str().unwrap()).await,
            StatusCode::NOT_FOUND,
            "task.not_found",
        )
        .await;
    }
    assert_problem(
        request(
            &server,
            &person,
            Method::GET,
            &format!("{TASKS}/TK-1"),
            Value::Null,
        )
        .await,
        StatusCode::NOT_FOUND,
        "task.not_found",
    )
    .await;
    assert_eq!(
        read(&server, &person, "TK-2").await["files"][0]["key"],
        "FL-4"
    );
    assert_exists(&server, &retained).await;
    server.stop().await;
}

#[tokio::test]
async fn register_agent_file_takes_new_key_when_earlier_agent_file_was_deleted() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Stable file keys").await;
    uploaded(&server, &agent, &task).await;
    assert_deleted(remove(&server, &person, SPACE, &task, "FL-1").await).await;
    assert_eq!(read(&server, &person, "TK-1").await["files"], json!([]));
    assert_eq!(register(&server, &agent, &task).await["key"], "FL-2");
    let next = create(&server, &agent, SPACE, "Another task").await;
    assert_eq!(register(&server, &agent, &next).await["key"], "FL-3");
    server.stop().await;
}

async fn uploaded(server: &Server, agent: &Value, task: &Value) -> Value {
    let agent_file = register(server, agent, task).await;
    upload_ok(server, &agent_file, "File bytes").await;
    assert_eq!(
        complete(server, agent, task, agent_file["key"].as_str().unwrap())
            .await
            .status(),
        StatusCode::OK
    );
    agent_file
}

async fn remove(server: &Server, tokens: &Value, space: &str, task: &Value, key: &str) -> Response {
    request(
        server,
        tokens,
        Method::DELETE,
        &format!(
            "/api/v1/spaces/{space}/tasks/{}/files/{key}",
            task["task_id"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await
}

async fn assert_deleted(response: Response) {
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data":null,"meta":{}})
    );
}

async fn assert_exists(server: &Server, agent_file: &Value) {
    assert_problem(
        upload(server, agent_file, "Existing file").await,
        StatusCode::PRECONDITION_FAILED,
        "file.already_exists",
    )
    .await;
}
