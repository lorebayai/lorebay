use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, assert_problem, authorize_for, list_page, login, request};
use crate::mcp::tools::{call, call_error};

use super::helpers::{SPACE, TASKS, create, read, setup, space_member};

#[tokio::test]
async fn update_task_keeps_result_versions_and_refuses_stale_replacements() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Versioned work").await;
    let path = format!("{TASKS}/TK-1/results");
    assert_unwritten(&server, &person, &path).await;
    let first = call(
        &server,
        &agent,
        "update_task",
        json!({
            "task_id":task["task_id"], "result":"First result"
        }),
    )
    .await;
    assert_eq!(first["result_version"], 1);
    let peer = authorize_for(&server, &person, "Continuing agent").await;
    call(
        &server,
        &peer,
        "claim_task",
        json!({"task_id":task["task_id"]}),
    )
    .await;
    let second = replace(
        &server,
        &peer,
        &task,
        json!({"result":"Second result", "result_version":1}),
    )
    .await;
    assert_eq!(second["result_version"], 2);
    refuse_stale(&server, &peer, &task, &second).await;
    refuse_version_without_result(&server, &peer, &task, &second).await;
    let third = call(&server, &peer, "update_task", json!({
        "task_id":task["task_id"], "result":"", "result_version":second["result_version"], "state":"done"
    })).await;
    assert_eq!(third["result_version"], 3);
    assert_eq!(third["result"], "");
    let history = list_page(&server, &person, &path, &[]).await;
    assert_eq!(
        history["data"],
        json!([
            {"version":3,"result":"","person":"alice","agent_name":"Continuing agent","created_at":third["last_activity_at"]},
            {"version":2,"result":"Second result","person":"alice","agent_name":"Continuing agent","created_at":second["last_activity_at"]},
            {"version":1,"result":"First result","person":"alice","agent_name":"Research agent","created_at":first["last_activity_at"]}
        ])
    );
    check_pages(&server, &peer, &history).await;
    let fourth = replace(
        &server,
        &peer,
        &task,
        json!({"result":"", "result_version":3}),
    )
    .await;
    assert_eq!(fourth["result_version"], 4);
    assert_eq!(fourth["state_changed_at"], third["state_changed_at"]);
    assert_eq!(fourth["ended_at"], third["ended_at"]);
    server.stop().await;
}

#[tokio::test]
async fn update_task_serializes_result_writes_at_same_version() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Concurrent work").await;
    let first = replace(
        &server,
        &agent,
        &task,
        json!({"result":"First", "result_version":null}),
    )
    .await;
    assert_eq!(first["result_version"], 1);
    let path = format!("{TASKS}/{}", task["task_id"].as_str().unwrap());
    let (left, right) = tokio::join!(
        request(
            &server,
            &agent,
            Method::PATCH,
            &path,
            json!({"result":"Left", "result_version":1})
        ),
        request(
            &server,
            &agent,
            Method::PATCH,
            &path,
            json!({"result":"Right", "result_version":1})
        )
    );
    let (success, conflict) = if left.status() == StatusCode::OK {
        (left, right)
    } else {
        (right, left)
    };
    assert_eq!(success.status(), StatusCode::OK);
    let winner = success.json::<Value>().await.unwrap()["data"].clone();
    assert_eq!(winner["result_version"], 2);
    assert_problem(conflict, StatusCode::CONFLICT, "task.result_changed").await;
    assert_eq!(read(&server, &person, "TK-1").await, winner);
    let history = list_page(&server, &person, &format!("{TASKS}/TK-1/results"), &[]).await;
    assert_eq!(history["data"].as_array().unwrap().len(), 2);
    assert_eq!(history["data"][0]["result"], winner["result"]);
    server.stop().await;
}

#[tokio::test]
async fn read_task_results_checks_membership_and_history_queries() {
    let (server, person, agent) = setup().await;
    let task = create(&server, &agent, SPACE, "Shared results").await;
    replace(&server, &agent, &task, json!({"result":"Shared"})).await;
    let bob = login(&server, "bob").await;
    let viewer = authorize_for(&server, &bob, "Reader").await;
    let path = format!("{TASKS}/TK-1/results");
    assert_problem(
        request(&server, &bob, Method::GET, &path, Value::Null).await,
        StatusCode::NOT_FOUND,
        "space.not_found",
    )
    .await;
    space_member(&server, &person, Method::POST, "viewer").await;
    for caller in [&bob, &viewer] {
        let history = list_page(&server, caller, &path, &[]).await;
        assert_eq!(history["data"][0]["result"], "Shared");
    }
    let task = call(
        &server,
        &viewer,
        "get_task",
        json!({
            "space":SPACE,"key":"TK-1","include_results":true
        }),
    )
    .await;
    assert_eq!(task["results"], list_page(&server, &bob, &path, &[]).await);
    check_history_queries(&server, &viewer).await;
    space_member(&server, &person, Method::DELETE, "").await;
    assert_problem(
        request(&server, &viewer, Method::GET, &path, Value::Null).await,
        StatusCode::NOT_FOUND,
        "space.not_found",
    )
    .await;
    server.stop().await;
}

async fn replace(server: &Server, agent: &Value, task: &Value, changes: Value) -> Value {
    let response = request(
        server,
        agent,
        Method::PATCH,
        &format!("{TASKS}/{}", task["task_id"].as_str().unwrap()),
        changes,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["data"].clone()
}

async fn refuse_stale(server: &Server, agent: &Value, task: &Value, expected: &Value) {
    for version in [json!(1), Value::Null] {
        let changes = json!({"result":"Lost update", "result_version":version, "state":"done", "title":"Lost title"});
        let response = request(
            server,
            agent,
            Method::PATCH,
            &format!("{TASKS}/{}", task["task_id"].as_str().unwrap()),
            changes.clone(),
        )
        .await;
        assert_problem(response, StatusCode::CONFLICT, "task.result_changed").await;
        let mut arguments = changes;
        arguments["task_id"] = task["task_id"].clone();
        assert_eq!(
            call_error(server, agent, "update_task", arguments).await["code"],
            "task.result_changed"
        );
    }
    assert_eq!(
        call_error(
            server,
            agent,
            "update_task",
            json!({
                "task_id":task["task_id"], "result":"Omitted version"
            })
        )
        .await["code"],
        "task.result_changed"
    );
    assert_eq!(&read(server, agent, "TK-1").await, expected);
}

async fn check_pages(server: &Server, agent: &Value, history: &Value) {
    let first = call(
        server,
        agent,
        "get_task",
        json!({
            "space":SPACE, "key":"TK-1", "include_results":true, "page_size":2
        }),
    )
    .await;
    let web = list_page(
        server,
        agent,
        &format!("{TASKS}/TK-1"),
        &[("include_results", "true"), ("page_size", "2")],
    )
    .await;
    assert_eq!(first, web["data"]);
    assert_eq!(
        first["results"]["data"],
        json!(history["data"].as_array().unwrap()[..2])
    );
    let cursor = first["results"]["meta"]["next_cursor"].as_str().unwrap();
    let last = call(
        server,
        agent,
        "get_task",
        json!({
            "space":SPACE, "key":"TK-1", "include_results":true, "page_size":2, "cursor":cursor
        }),
    )
    .await;
    let web = list_page(
        server,
        agent,
        &format!("{TASKS}/TK-1/results"),
        &[("page_size", "2"), ("cursor", cursor)],
    )
    .await;
    assert_eq!(last["results"], web);
    assert_eq!(last["results"]["data"], json!([history["data"][2]]));
    assert_eq!(last["results"]["meta"], json!({}));
}

async fn assert_unwritten(server: &Server, person: &Value, path: &str) {
    assert_eq!(
        list_page(server, person, path, &[]).await["data"],
        json!([])
    );
    assert_eq!(read(server, person, "TK-1").await["result_version"], 0);
}

async fn check_history_queries(server: &Server, agent: &Value) {
    for query in [
        "include_results=true&include_scratchpad=true",
        "include_results=true&cursor=invalid",
        "include_results=true&page_size=0",
    ] {
        assert_problem(
            request(
                server,
                agent,
                Method::GET,
                &format!("{TASKS}/TK-1?{query}"),
                Value::Null,
            )
            .await,
            StatusCode::BAD_REQUEST,
            "request.invalid_query",
        )
        .await;
    }
    for arguments in [
        json!({"include_results":true,"include_scratchpad":true}),
        json!({"include_results":true,"cursor":"invalid"}),
        json!({"include_results":true,"page_size":0}),
    ] {
        let mut arguments = arguments;
        arguments["space"] = json!(SPACE);
        arguments["key"] = json!("TK-1");
        assert_eq!(
            call_error(server, agent, "get_task", arguments).await["code"],
            "request.invalid_query"
        );
    }
    assert_problem(
        request(
            server,
            agent,
            Method::GET,
            &format!("{TASKS}/TK-1/results?cursor=invalid"),
            Value::Null,
        )
        .await,
        StatusCode::BAD_REQUEST,
        "request.invalid_query",
    )
    .await;
}

async fn refuse_version_without_result(
    server: &Server,
    agent: &Value,
    task: &Value,
    expected: &Value,
) {
    assert_problem(
        request(
            server,
            agent,
            Method::PATCH,
            &format!("{TASKS}/{}", task["task_id"].as_str().unwrap()),
            json!({"title":"Must not apply", "result_version":2}),
        )
        .await,
        StatusCode::BAD_REQUEST,
        "request.invalid_body",
    )
    .await;
    assert_eq!(
        call_error(
            server,
            agent,
            "update_task",
            json!({"task_id":task["task_id"],"title":"Must not apply","result_version":2})
        )
        .await["code"],
        "request.invalid_body"
    );
    assert_eq!(&read(server, agent, "TK-1").await, expected);
}
