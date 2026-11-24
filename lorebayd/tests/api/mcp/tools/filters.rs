use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::{Server, authorize_for, list_page, list_response, login};
use crate::tasks::helpers::{self as tasks, SPACE, TASKS};

use super::{call, call_error};

#[tokio::test]
async fn call_list_tasks_returns_own_party_tasks_not_ended() {
    let (server, person, agent) = tasks::setup().await;
    let peer = authorize_for(&server, &person, "Continuing agent").await;
    tasks::create(&server, &agent, SPACE, "Own first").await;
    tasks::create(&server, &peer, SPACE, "Own second").await;
    let ended = tasks::create(&server, &agent, SPACE, "Own ended").await;
    call(
        &server,
        &agent,
        "update_task",
        json!({"task_id":ended["task_id"], "state":"done"}),
    )
    .await;
    let bob_agent = add_bob_agent(&server, &person).await;
    tasks::create(&server, &bob_agent, SPACE, "Other party").await;
    let mut arguments = json!({"space":SPACE, "own_open":true, "page_size":1});
    let mut rows = Vec::new();
    loop {
        let page = call(&server, &agent, "list_tasks", arguments.clone()).await;
        rows.extend(page["data"].as_array().unwrap().iter().cloned());
        if let Some(cursor) = page["meta"]["next_cursor"].as_str() {
            assert!(rows.len() < 2);
            arguments["cursor"] = json!(cursor);
        } else {
            break;
        }
    }
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .map(|row| row["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Own second", "Own first"]
    );
    assert!(
        rows.iter()
            .all(|row| row["person"] == "alice" && row["state"] == "working")
    );
    assert_party_tasks(&server, &agent).await;
    recover_own_task(&server, &person, &agent, &rows[0]).await;
    let page = call(
        &server,
        &agent,
        "list_tasks",
        json!({"space":SPACE,"own_open":true,"agent":"Research agent","keyword":"first"}),
    )
    .await;
    assert_eq!(page["data"].as_array().unwrap().len(), 1);
    assert_eq!(page["data"][0]["title"], "Own first");
    server.stop().await;
}

#[tokio::test]
async fn call_list_tasks_applies_filters_and_binds_cursors() {
    let (server, person, agent) = tasks::setup().await;
    tasks::create(&server, &agent, SPACE, "Shared alpha").await;
    tasks::create(&server, &agent, SPACE, "Shared beta").await;
    let ended = tasks::create(&server, &agent, SPACE, "Shared ended").await;
    call(
        &server,
        &agent,
        "update_task",
        json!({"task_id":ended["task_id"],"state":"done","result":"Needle in result"}),
    )
    .await;
    let bob_agent = add_bob_agent(&server, &person).await;
    tasks::create(&server, &bob_agent, SPACE, "Shared bob first").await;
    tasks::create(&server, &bob_agent, SPACE, "Shared bob second").await;
    for (field, initial, changed) in [
        ("person", json!("alice"), json!("bob")),
        ("agent", json!("Research agent"), json!("Unknown agent")),
        ("state", json!("working"), json!("done")),
        ("keyword", json!("Shared"), json!("Needle")),
        ("own_open", json!(true), json!(false)),
    ] {
        assert_bound_cursor(&server, &agent, field, initial, changed).await;
    }
    for (filters, count) in [
        (json!({"person":"bob","agent":"Research agent"}), 2),
        (json!({"agent":"Research agent"}), 5),
        (json!({"state":"done"}), 1),
        (json!({"keyword":"Needle"}), 1),
        (json!({"person":"unknown"}), 0),
    ] {
        let mut arguments = filters;
        arguments["space"] = json!(SPACE);
        let page = call(&server, &agent, "list_tasks", arguments.clone()).await;
        assert_eq!(page["data"].as_array().unwrap().len(), count);
        assert_api_page(&server, &agent, &arguments, &page).await;
    }
    server.stop().await;
}

#[tokio::test]
async fn call_list_tasks_fails_when_filters_are_invalid() {
    let (server, person, agent) = tasks::setup().await;
    tasks::create(&server, &agent, SPACE, "Unchanged").await;
    let original = tasks::read(&server, &person, "TK-1").await;
    for filters in [
        json!({"keyword":""}),
        json!({"keyword":"   "}),
        json!({"keyword":"x".repeat(201)}),
        json!({"state":"closed"}),
        json!({"own_open":true,"person":"alice"}),
        json!({"own_open":true,"state":"working"}),
        json!({"own_open":"yes"}),
        json!({"unknown":"private-sentinel"}),
    ] {
        let mut arguments = filters.clone();
        arguments["space"] = json!(SPACE);
        let error = call_error(&server, &agent, "list_tasks", arguments).await;
        let query = query(&filters);
        let pairs = query
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let response = list_response(TASKS, &server, &agent, &pairs).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let problem = response.json::<Value>().await.unwrap();
        assert_eq!(error["code"], problem["code"]);
        assert_eq!(error["message"], problem["title"]);
        assert_eq!(error["errors"], problem["errors"]);
        assert!(!error.to_string().contains("private-sentinel"));
    }
    assert_eq!(tasks::read(&server, &person, "TK-1").await, original);
    server.stop().await;
}

async fn add_bob_agent(server: &Server, person: &Value) -> Value {
    let bob = login(server, "bob").await;
    tasks::space_member(server, person, Method::POST, "contributor").await;
    authorize_for(server, &bob, "Research agent").await
}

async fn assert_party_tasks(server: &Server, agent: &Value) {
    let spaces = call(server, agent, "list_spaces", json!({})).await;
    let person = &spaces["data"][0]["caller_person"];
    assert_eq!(person, "alice");
    let arguments = json!({"space":SPACE, "person":person});
    let page = call(server, agent, "list_tasks", arguments.clone()).await;
    assert_api_page(server, agent, &arguments, &page).await;
    let rows = page["data"].as_array().unwrap();
    let mut titles = rows
        .iter()
        .map(|row| row["title"].as_str().unwrap())
        .collect::<Vec<_>>();
    titles.sort_unstable();
    assert_eq!(titles, ["Own ended", "Own first", "Own second"]);
    assert!(rows.iter().all(|row| row["person"] == *person));
    assert!(
        rows.iter()
            .any(|row| row["title"] == "Own ended" && row["state"] == "done")
    );
}

async fn assert_bound_cursor(
    server: &Server,
    agent: &Value,
    field: &str,
    initial: Value,
    changed: Value,
) {
    let mut arguments = json!({"space":SPACE,"page_size":1});
    arguments[field] = initial;
    let first = call(server, agent, "list_tasks", arguments.clone()).await;
    assert_api_page(server, agent, &arguments, &first).await;
    assert_eq!(first["data"].as_array().unwrap().len(), 1);
    arguments["cursor"] = first["meta"]["next_cursor"].clone();
    assert!(arguments["cursor"].is_string());
    let next = call(server, agent, "list_tasks", arguments.clone()).await;
    assert_api_page(server, agent, &arguments, &next).await;
    assert_ne!(first["data"][0]["task_id"], next["data"][0]["task_id"]);
    arguments[field] = changed;
    let error = call_error(server, agent, "list_tasks", arguments).await;
    assert_eq!(error["code"], "request.invalid_query");
    assert_eq!(error["errors"][0]["field"], "cursor");
}

async fn assert_api_page(server: &Server, agent: &Value, arguments: &Value, page: &Value) {
    let query = query(arguments);
    let pairs = query
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(*page, list_page(server, agent, TASKS, &pairs).await);
}

fn query(arguments: &Value) -> Vec<(String, String)> {
    arguments
        .as_object()
        .unwrap()
        .iter()
        .filter(|(key, _)| key.as_str() != "space")
        .map(|(key, value)| {
            (
                key.clone(),
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            )
        })
        .collect()
}

async fn recover_own_task(server: &Server, person: &Value, agent: &Value, task: &Value) {
    call(
        server,
        agent,
        "claim_task",
        json!({"task_id":task["task_id"]}),
    )
    .await;
    let checkpoint = call(
        server,
        agent,
        "add_checkpoint",
        json!({"task_id":task["task_id"], "content":"Recovered own task"}),
    )
    .await;
    assert_eq!(checkpoint["task_title"], "Own second");
    assert_eq!(
        tasks::scratchpad(server, person, "TK-2").await,
        json!([{"time":checkpoint["time"], "content":"Recovered own task"}])
    );
}
