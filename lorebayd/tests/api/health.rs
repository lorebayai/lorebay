use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use reqwest::{Client, StatusCode};
use sea_orm::sea_query::{
    Alias, ColumnDef, Expr, ForeignKey, ForeignKeyAction, Index, Query, Table, TableCreateStatement,
};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection, ExprTrait};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;

use crate::helpers::{Server, TEST_TIMEOUT};

#[tokio::test]
async fn check_health_succeeds_when_server_is_running() {
    let server = Server::start().await;
    for path in ["/healthz", "/readyz"] {
        let response = server.get(path).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({"data": "ok", "meta": {}})
        );
    }
    let directory = server.stop().await;
    let restarted = Server::restart(directory).await;
    assert_eq!(restarted.get("/readyz").await.status(), StatusCode::OK);
    restarted.stop().await;
}

#[tokio::test]
async fn check_readiness_fails_when_shutdown_has_begun() {
    let mut server = Server::start().await;
    server.begin_shutdown();
    server
        .wait_for_status("/readyz", StatusCode::SERVICE_UNAVAILABLE)
        .await;
    let client = Client::builder()
        .pool_max_idle_per_host(0)
        .timeout(TEST_TIMEOUT)
        .no_proxy()
        .build()
        .unwrap();
    let response = client
        .get(format!("http://{}/readyz", server.address))
        .send()
        .await
        .expect("new connection after shutdown receives readiness response");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({
            "type": "about:blank", "title": "Service Unavailable", "status": 503,
            "code": "server.not_ready", "errors": []
        })
    );
    let response = client
        .get(format!("http://{}/healthz", server.address))
        .send()
        .await
        .expect("new connection after shutdown receives liveness response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data": "ok", "meta": {}})
    );
    server.finish_shutdown().await;
}

#[tokio::test]
async fn shut_down_completes_in_flight_request() {
    let mut server = Server::start().await;
    let mut upload = server.begin_upload().await;
    server.begin_shutdown();
    server
        .wait_for_status("/readyz", StatusCode::SERVICE_UNAVAILABLE)
        .await;
    server.wait_for_listener_to_close().await;
    let completion = async {
        let mut response = String::new();
        upload.write_all(b"{}").await.unwrap();
        upload.read_to_string(&mut response).await.unwrap();
        assert!(
            response.starts_with("HTTP/1.1 404 Not Found\r\n"),
            "{response}"
        );
        let body = response.split_once("\r\n\r\n").unwrap().1;
        assert_eq!(
            serde_json::from_str::<Value>(body).unwrap(),
            json!({
                "type": "about:blank", "title": "Not Found", "status": 404,
                "code": "http.not_found", "errors": []
            })
        );
    };
    timeout(TEST_TIMEOUT, completion).await.unwrap();
    server.finish_shutdown().await;
}

#[tokio::test]
async fn shut_down_finishes_when_client_stalls() {
    let mut server = Server::start().await;
    let mut upload = server.begin_upload().await;
    let started = Instant::now();
    server.begin_shutdown();
    server
        .wait_for_status("/readyz", StatusCode::SERVICE_UNAVAILABLE)
        .await;
    server.finish_shutdown().await;
    assert!(started.elapsed() >= Duration::from_secs(10));
    let mut byte = [0];
    let read = timeout(TEST_TIMEOUT, upload.read(&mut byte)).await.unwrap();
    assert!(
        matches!(read, Ok(0) | Err(_)),
        "stalled connection remains open"
    );
}

#[tokio::test]
async fn startup_refuses_earlier_data_directory_without_changing_database() {
    let directory = TempDir::new().unwrap();
    let data = directory.path().join("data");
    fs::create_dir(&data).unwrap();
    let path = data.join("lorebay.sqlite3");
    let filename = path.clone();
    let mut options = ConnectOptions::new("sqlite://");
    options.map_sqlx_sqlite_opts(move |sqlite| sqlite.filename(&filename).create_if_missing(true));
    let database = Database::connect(options).await.unwrap();
    let history = Alias::new("seaql_migrations");
    let version = Alias::new("version");
    let applied_at = Alias::new("applied_at");
    let table = Table::create()
        .table(history.clone())
        .col(
            ColumnDef::new(version.clone())
                .text()
                .not_null()
                .primary_key(),
        )
        .col(ColumnDef::new(applied_at.clone()).big_integer().not_null())
        .to_owned();
    database.execute(&table).await.unwrap();
    let row = Query::insert()
        .into_table(history)
        .columns([version, applied_at])
        .values_panic(["m20261116_000001_create_tables".into(), 0_i64.into()])
        .to_owned();
    database.execute(&row).await.unwrap();
    database.close().await.unwrap();
    let bytes = fs::read(&path).unwrap();
    let error = Server::startup_error(directory.path()).await;
    assert!(error.contains(data.to_str().unwrap()), "{error}");
    assert!(
        error.contains("was created by an earlier lorebayd build and must be recreated"),
        "{error}"
    );
    assert!(
        error.contains("Move it aside and restart with an empty data directory"),
        "{error}"
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[tokio::test]
async fn startup_refuses_data_directory_without_refused_artifact_column() {
    let directory = Server::start().await.stop().await;
    let data = directory.path().join("data");
    let path = data.join("lorebay.sqlite3");
    let database = connect_without_foreign_keys(&path).await;
    replace_artifact_table_with_earlier_schema(&database).await;
    database.close().await.unwrap();
    let bytes = fs::read(&path).unwrap();
    let error = Server::startup_error(directory.path()).await;
    assert!(error.contains(data.to_str().unwrap()), "{error}");
    assert!(
        error.contains("was created by an earlier lorebayd build and must be recreated"),
        "{error}"
    );
    assert!(
        error.contains("Move it aside and restart with an empty data directory"),
        "{error}"
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}

async fn connect_without_foreign_keys(path: &Path) -> DatabaseConnection {
    let filename = path.to_path_buf();
    let mut options = ConnectOptions::new("sqlite://");
    options.map_sqlx_sqlite_opts(move |sqlite| sqlite.filename(&filename).foreign_keys(false));
    Database::connect(options).await.unwrap()
}

async fn replace_artifact_table_with_earlier_schema(database: &DatabaseConnection) {
    let artifact = Alias::new("space_artifact");
    database
        .execute(&Table::drop().table(artifact.clone()).to_owned())
        .await
        .unwrap();
    database.execute(&earlier_artifact_table()).await.unwrap();
    let index = Index::create()
        .name("space_artifact_task")
        .table(artifact)
        .col(Alias::new("task_id"))
        .to_owned();
    database.execute(&index).await.unwrap();
}

fn earlier_artifact_table() -> TableCreateStatement {
    let artifact = Alias::new("space_artifact");
    let [space_id, task_id, number, size, completed_at] =
        ["space_id", "task_id", "number", "size", "completed_at"].map(Alias::new);
    Table::create()
        .table(artifact.clone())
        .col(
            ColumnDef::new(Alias::new("id"))
                .text()
                .not_null()
                .primary_key(),
        )
        .col(ColumnDef::new(space_id.clone()).text().not_null())
        .col(ColumnDef::new(task_id.clone()).text().not_null())
        .col(ColumnDef::new(number.clone()).big_integer().not_null())
        .col(ColumnDef::new(Alias::new("file_name")).text().not_null())
        .col(ColumnDef::new(Alias::new("summary")).text().not_null())
        .col(
            ColumnDef::new(Alias::new("storage_key"))
                .text()
                .not_null()
                .unique_key(),
        )
        .col(ColumnDef::new(size.clone()).big_integer())
        .col(
            ColumnDef::new(Alias::new("created_at"))
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(completed_at.clone()).big_integer())
        .index(Index::create().unique().col(space_id.clone()).col(number))
        .check(
            Expr::col(completed_at)
                .is_null()
                .eq(Expr::col(size).is_null()),
        )
        .foreign_key(
            ForeignKey::create()
                .from(artifact.clone(), space_id.clone())
                .to(Alias::new("space"), Alias::new("id")),
        )
        .foreign_key(
            ForeignKey::create()
                .from(artifact, (task_id, space_id))
                .to(
                    Alias::new("space_task"),
                    (Alias::new("id"), Alias::new("space_id")),
                )
                .on_delete(ForeignKeyAction::Cascade),
        )
        .to_owned()
}
