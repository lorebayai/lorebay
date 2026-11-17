use reqwest::{Method, Response, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{
    Server, assert_problem, authorize_for, create_space, list_page, login, request,
};

use super::helpers::{
    SPACE, TASKS, append, create, note_request, participant, read, scratchpad, setup,
};

#[tokio::test]
async fn delete_task_removes_task_and_scratchpad() {
    let (server, owner, _) = setup().await;
    let person = login(&server, "bob").await;
    participant(&server, &owner, Method::POST, "contributor").await;
    let agent = authorize_for(&server, &person, "Bob agent").await;
    let task = create(&server, &agent, SPACE, "Mistaken task").await;
    let note = append(&server, &agent, &task, "Mistaken note").await;
    let artifact = prepare_artifacts(&server, &agent, &task).await;
    let id = task["task_id"].as_str().unwrap();
    assert_eq!(
        scratchpad(&server, &person, "TK-1").await[0]["time"],
        note["time"]
    );
    for time in [None, Some(note["time"].to_string())] {
        assert_problem(
            remove(&server, &person, SPACE, "TK-1", time.as_deref()).await,
            StatusCode::NOT_FOUND,
            "task.not_found",
        )
        .await;
    }
    assert_deleted(remove(&server, &person, SPACE, id, None).await).await;
    assert_eq!(
        list_page(&server, &person, TASKS, &[]).await["data"],
        json!([])
    );
    for path in [format!("{TASKS}/TK-1"), format!("{TASKS}/TK-1/scratchpad")] {
        assert_problem(
            request(&server, &person, Method::GET, &path, Value::Null).await,
            StatusCode::NOT_FOUND,
            "task.not_found",
        )
        .await;
    }
    for time in [None, Some(note["time"].to_string())] {
        assert_problem(
            remove(&server, &person, SPACE, id, time.as_deref()).await,
            StatusCode::NOT_FOUND,
            "task.not_found",
        )
        .await;
    }
    let next = create(&server, &agent, SPACE, "Replacement").await;
    assert_eq!(next["key"], "TK-2");
    assert_ne!(next["task_id"], task["task_id"]);
    assert_eq!(scratchpad(&server, &person, "TK-2").await, json!([]));
    assert_problem(
        upload_artifact(&server, &artifact).await,
        StatusCode::PRECONDITION_FAILED,
        "file.already_exists",
    )
    .await;
    server.stop().await;
}

#[tokio::test]
async fn add_note_fails_with_not_found_when_task_was_deleted() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Deleted task").await;
    let id = task["task_id"].as_str().unwrap();
    assert_deleted(remove(&server, &person, SPACE, id, None).await).await;
    assert_problem(
        note_request(&server, &agent, SPACE, id, "Too late").await,
        StatusCode::NOT_FOUND,
        "task.not_found",
    )
    .await;
    assert_eq!(
        list_page(&server, &person, TASKS, &[]).await["data"],
        json!([])
    );
    server.stop().await;
}

#[tokio::test]
async fn delete_task_fails_when_caller_is_agent() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Retained task").await;
    let note = append(&server, &agent, &task, "Retained note").await;
    let id = task["task_id"].as_str().unwrap();
    let time = note["time"].to_string();
    let original = read(&server, &person, "TK-1").await;
    for space in [SPACE, "SPACE-999"] {
        for task_id in [id, "unknown"] {
            for time in [None, Some(time.as_str())] {
                assert_problem(
                    remove(&server, &agent, space, task_id, time).await,
                    StatusCode::FORBIDDEN,
                    "auth.person_required",
                )
                .await;
            }
        }
    }
    assert_eq!(read(&server, &person, "TK-1").await, original);
    assert_eq!(
        scratchpad(&server, &person, "TK-1").await[0]["time"],
        note["time"]
    );
    server.stop().await;
}

#[tokio::test]
async fn delete_task_fails_when_task_belongs_to_another_person() {
    let (server, person, agent) = setup().await;
    let bob = login(&server, "bob").await;
    let task = create(&server, &agent, SPACE, "Alice task").await;
    let note = append(&server, &agent, &task, "Alice note").await;
    let id = task["task_id"].as_str().unwrap();
    let time = note["time"].to_string();
    let original = read(&server, &person, "TK-1").await;
    participant(&server, &person, Method::POST, "contributor").await;
    for time in [None, Some(time.as_str())] {
        assert_problem(
            remove(&server, &bob, SPACE, id, time).await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
    }
    participant(&server, &person, Method::DELETE, "").await;
    create_space(&server, &bob, "Bob only", "Private work").await;
    for time in [None, Some(time.as_str())] {
        assert_problem(
            remove(&server, &bob, "SPACE-2", id, time).await,
            StatusCode::NOT_FOUND,
            "task.not_found",
        )
        .await;
        let unknown = assert_problem(
            remove(&server, &bob, SPACE, "unknown", time).await,
            StatusCode::NOT_FOUND,
            "space.not_found",
        )
        .await;
        assert_eq!(
            assert_problem(
                remove(&server, &bob, SPACE, id, time).await,
                StatusCode::NOT_FOUND,
                "space.not_found",
            )
            .await,
            unknown
        );
    }
    assert_eq!(read(&server, &person, "TK-1").await, original);
    assert_eq!(
        scratchpad(&server, &person, "TK-1").await[0]["time"],
        note["time"]
    );
    server.stop().await;
}

#[tokio::test]
async fn delete_task_fails_when_person_became_viewer() {
    let (server, owner, _) = setup().await;
    let person = login(&server, "bob").await;
    participant(&server, &owner, Method::POST, "contributor").await;
    let agent = authorize_for(&server, &person, "Bob agent").await;
    let task = create(&server, &agent, SPACE, "Bob task").await;
    let note = append(&server, &agent, &task, "Bob note").await;
    let original = read(&server, &person, "TK-1").await;
    participant(&server, &owner, Method::PATCH, "viewer").await;
    for time in [None, Some(note["time"].to_string())] {
        assert_problem(
            remove(
                &server,
                &person,
                SPACE,
                task["task_id"].as_str().unwrap(),
                time.as_deref(),
            )
            .await,
            StatusCode::FORBIDDEN,
            "space.permission_required",
        )
        .await;
    }
    assert_eq!(read(&server, &person, "TK-1").await, original);
    assert_eq!(
        scratchpad(&server, &person, "TK-1").await[0]["time"],
        note["time"]
    );
    server.stop().await;
}

#[tokio::test]
async fn delete_task_removes_another_persons_task_when_caller_is_owner() {
    let (server, owner, owner_agent) = setup().await;
    let person = login(&server, "bob").await;
    participant(&server, &owner, Method::POST, "contributor").await;
    let agent = authorize_for(&server, &person, "Bob agent").await;
    let task = create(&server, &agent, SPACE, "Bob task").await;
    let note = append(&server, &agent, &task, "Bob note").await;
    let id = task["task_id"].as_str().unwrap();
    let time = note["time"].to_string();
    assert_deleted(remove(&server, &owner, SPACE, id, Some(&time)).await).await;
    participant(&server, &owner, Method::DELETE, "").await;
    assert_deleted(remove(&server, &owner, SPACE, id, None).await).await;
    assert_eq!(
        list_page(&server, &owner, TASKS, &[]).await["data"],
        json!([])
    );
    participant(&server, &owner, Method::POST, "contributor").await;
    participant(&server, &owner, Method::PATCH, "admin").await;
    let task = create(&server, &owner_agent, SPACE, "Alice task").await;
    let note = append(&server, &owner_agent, &task, "Alice note").await;
    let id = task["task_id"].as_str().unwrap();
    let time = note["time"].to_string();
    assert_deleted(remove(&server, &person, SPACE, id, Some(&time)).await).await;
    assert_eq!(scratchpad(&server, &owner, "TK-2").await, json!([]));
    assert_eq!(read(&server, &owner, "TK-2").await["person"], "alice");
    assert_deleted(remove(&server, &person, SPACE, id, None).await).await;
    assert_eq!(
        list_page(&server, &owner, TASKS, &[]).await["data"],
        json!([])
    );
    server.stop().await;
}

#[tokio::test]
async fn delete_note_removes_one_note_from_scratchpad() {
    let (server, owner, _) = setup().await;
    let person = login(&server, "bob").await;
    participant(&server, &owner, Method::POST, "contributor").await;
    let agent = authorize_for(&server, &person, "Bob agent").await;
    let task = create(&server, &agent, SPACE, "Notes").await;
    for content in ["First", "Middle", "Last"] {
        append(&server, &agent, &task, content).await;
    }
    let notes = scratchpad(&server, &person, "TK-1").await;
    let original = read(&server, &person, "TK-1").await;
    let id = task["task_id"].as_str().unwrap();
    let time = notes[1]["time"].to_string();
    assert_deleted(remove(&server, &person, SPACE, id, Some(&time)).await).await;
    assert_eq!(
        scratchpad(&server, &person, "TK-1").await,
        json!([notes[0], notes[2]])
    );
    for missing in [time, "0".to_owned()] {
        assert_problem(
            remove(&server, &person, SPACE, id, Some(&missing)).await,
            StatusCode::NOT_FOUND,
            "note.not_found",
        )
        .await;
    }
    assert_invalid_times(&server, &person, id).await;
    assert_eq!(
        scratchpad(&server, &person, "TK-1").await,
        json!([notes[0], notes[2]])
    );
    assert_eq!(read(&server, &person, "TK-1").await, original);
    for note in [&notes[0], &notes[2]] {
        assert_deleted(remove(&server, &person, SPACE, id, Some(&note["time"].to_string())).await)
            .await;
    }
    assert_eq!(scratchpad(&server, &person, "TK-1").await, json!([]));
    update_result(&server, &person, id).await;
    let next = append(&server, &agent, &task, "After deletion").await;
    assert!(next["time"].as_i64().unwrap() > notes[2]["time"].as_i64().unwrap());
    server.stop().await;
}

async fn assert_invalid_times(server: &Server, tokens: &Value, id: &str) {
    for invalid in ["not-a-number", "1.5", "9223372036854775808"] {
        let problem = assert_problem(
            remove(server, tokens, SPACE, id, Some(invalid)).await,
            StatusCode::BAD_REQUEST,
            "request.invalid_path",
        )
        .await;
        assert_eq!(problem["errors"][0]["field"], "time");
    }
}

async fn update_result(server: &Server, tokens: &Value, id: &str) {
    let response = request(
        server,
        tokens,
        Method::PATCH,
        &format!("{TASKS}/{id}"),
        json!({"result":"Notes removed"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn prepare_artifacts(server: &Server, agent: &Value, task: &Value) -> Value {
    let completed = register_artifact(server, agent, task).await;
    register_artifact(server, agent, task).await;
    assert_eq!(
        upload_artifact(server, &completed).await.status(),
        StatusCode::CREATED
    );
    let response = request(
        server,
        agent,
        Method::POST,
        &format!(
            "{TASKS}/{}/artifacts/{}/complete",
            task["task_id"].as_str().unwrap(),
            completed["key"].as_str().unwrap()
        ),
        Value::Null,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let shown = read(server, agent, task["key"].as_str().unwrap()).await;
    assert_eq!(shown["artifacts"].as_array().unwrap().len(), 1);
    assert_eq!(shown["artifacts"][0]["key"], completed["key"]);
    completed
}

async fn register_artifact(server: &Server, agent: &Value, task: &Value) -> Value {
    let response = request(
        server,
        agent,
        Method::POST,
        &format!("{TASKS}/{}/artifacts", task["task_id"].as_str().unwrap()),
        json!({"file_name":"retained.txt", "summary":"Retained task file"}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json::<Value>().await.unwrap()["data"].clone()
}

async fn upload_artifact(server: &Server, artifact: &Value) -> Response {
    let mut request = server
        .client
        .put(artifact["upload_url"].as_str().unwrap())
        .body("Retained task file");
    for (name, value) in artifact["headers"].as_object().unwrap() {
        request = request.header(name, value.as_str().unwrap());
    }
    request.send().await.unwrap()
}

async fn remove(
    server: &Server,
    tokens: &Value,
    space: &str,
    id: &str,
    time: Option<&str>,
) -> Response {
    let mut path = format!("/api/v1/spaces/{space}/tasks/{id}");
    if let Some(time) = time {
        path.push_str(&format!("/notes/{time}"));
    }
    request(server, tokens, Method::DELETE, &path, Value::Null).await
}

async fn assert_deleted(response: Response) {
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data":null,"meta":{}})
    );
}
