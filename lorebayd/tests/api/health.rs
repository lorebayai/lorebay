use std::collections::BTreeMap;
use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use reqwest::{Client, StatusCode};
use sea_orm::sea_query::{Alias, ColumnDef, Query, Table};
use sea_orm::{ConnectOptions, ConnectionTrait, Database, DatabaseConnection};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;

use crate::helpers::{
    STARTUP_TIMEOUT, Server, TEST_PASSWORD, TEST_TIMEOUT, TEST_USERNAME, write_users,
};

#[tokio::test]
async fn readiness_command_reports_ready_and_unreachable_servers() {
    let server = Server::start().await;
    let config = server.directory.path().join("probe.json");
    let dotenv = server.directory.path().join("probe.env");
    fs::write(
        &config,
        json!({"users_file": "/unused/users", "listen_address": "127.0.0.1:0"}).to_string(),
    )
    .unwrap();
    fs::write(
        &dotenv,
        format!(
            "LOREBAYD__LISTEN_ADDRESS=0.0.0.0:{}\n",
            server.address.port()
        ),
    )
    .unwrap();
    let probe = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lorebayd"));
        command
            .args(["readiness", "--config"])
            .arg(&config)
            .arg("--dotenv")
            .arg(&dotenv)
            .env_clear()
            .env("TOKIO_WORKER_THREADS", "2");
        command
    };
    let ready = timeout(TEST_TIMEOUT, probe().output())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        ready.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ready.stderr)
    );
    let _directory = server.stop().await;
    let unreachable = timeout(TEST_TIMEOUT, probe().output())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(unreachable.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&unreachable.stderr).contains("could not request /readyz"));
}

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
async fn startup_creates_data_directory_private_to_its_user() {
    let directory = TempDir::new().unwrap();
    let data = directory.path().join("missing-parent/data");
    let database = data.join("lorebay.sqlite3");
    let config = executable_config(directory.path(), &data).await;
    let mut keys = None;
    assert!(!data.exists());
    for (directory_mode, database_mode) in [(0o700, 0o600), (0o755, 0o644)] {
        if data.exists() {
            fs::set_permissions(&data, Permissions::from_mode(directory_mode)).unwrap();
            fs::set_permissions(&database, Permissions::from_mode(database_mode)).unwrap();
        }
        let mut child = start_executable(&config).await;
        for (path, expected) in [
            (data.clone(), directory_mode),
            (database.clone(), database_mode),
            (data.join("signing.key"), 0o600),
            (data.join("file-url-signing.key"), 0o600),
        ] {
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, expected, "{}", path.display());
        }
        stop_executable(&mut child).await;
        let written =
            ["signing.key", "file-url-signing.key"].map(|name| fs::read(data.join(name)).unwrap());
        if let Some(previous) = &keys {
            assert_eq!(&written, previous);
        }
        keys = Some(written);
    }
}

#[tokio::test]
async fn startup_refuses_earlier_data_directory_without_changing_database() {
    let directory = TempDir::new().unwrap();
    write_migration_histories(
        directory.path(),
        &[("seaql_migrations", "m20261116_000001_create_tables")],
    )
    .await;
    assert_refused_directory_unchanged(directory.path()).await;
}

#[tokio::test]
async fn startup_refuses_v01_data_directory_without_changing_database() {
    let histories = [
        ("space_migration", "m20261118_000001_create_space_tables"),
        ("seaql_migrations", "m20261118_000001_create_product_tables"),
    ];
    for recorded in [&histories[..], &histories[..1], &histories[1..]] {
        let directory = TempDir::new().unwrap();
        write_migration_histories(directory.path(), recorded).await;
        assert_refused_directory_unchanged(directory.path()).await;
    }
}

#[tokio::test]
async fn startup_refuses_data_directory_from_earlier_release() {
    let directory = Server::start().await.stop().await;
    let data = directory.path().join("data");
    let path = data.join("lorebay.sqlite3");
    let database = connect_without_foreign_keys(&path).await;
    database
        .execute(
            &Table::rename()
                .table(Alias::new("space_agent_file"), Alias::new("space_artifact"))
                .to_owned(),
        )
        .await
        .unwrap();
    database.close().await.unwrap();
    assert_refused_directory_unchanged(directory.path()).await;
}

async fn write_migration_histories(directory: &Path, histories: &[(&str, &str)]) {
    let data = directory.join("data");
    fs::create_dir(&data).unwrap();
    let filename = data.join("lorebay.sqlite3");
    let mut options = ConnectOptions::new("sqlite://");
    options.map_sqlx_sqlite_opts(move |sqlite| sqlite.filename(&filename).create_if_missing(true));
    let database = Database::connect(options).await.unwrap();
    for (history, migration) in histories {
        let history = Alias::new(*history);
        let version = Alias::new("version");
        let applied_at = Alias::new("applied_at");
        database
            .execute(
                &Table::create()
                    .table(history.clone())
                    .col(
                        ColumnDef::new(version.clone())
                            .text()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(applied_at.clone()).big_integer().not_null())
                    .to_owned(),
            )
            .await
            .unwrap();
        database
            .execute(
                &Query::insert()
                    .into_table(history)
                    .columns([version, applied_at])
                    .values_panic([(*migration).into(), 0_i64.into()])
                    .to_owned(),
            )
            .await
            .unwrap();
    }
    database.close().await.unwrap();
}

async fn assert_refused_directory_unchanged(directory: &Path) {
    let data = directory.join("data");
    let before = directory_contents(&data);
    let config = executable_config(directory, &data).await;
    let output = timeout(
        TEST_TIMEOUT,
        Command::new(env!("CARGO_BIN_EXE_lorebayd"))
            .arg("--config")
            .arg(&config)
            .env_clear()
            .env("TOKIO_WORKER_THREADS", "2")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains(data.to_str().unwrap()), "{error}");
    assert!(
        error.contains("was written by an earlier release"),
        "{error}"
    );
    assert!(
        error.contains("Delete it and restart with an empty data directory; the users file recreates the accounts"),
        "{error}"
    );
    let after = directory_contents(&data);
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        before.keys().collect::<Vec<_>>()
    );
    for (path, bytes) in before {
        assert!(after[&path] == bytes, "{} changed", path.display());
    }
}

fn directory_contents(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut contents = BTreeMap::new();
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let name = PathBuf::from(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            contents.insert(name.clone(), Vec::new());
            for (child, bytes) in directory_contents(&entry.path()) {
                contents.insert(name.join(child), bytes);
            }
        } else {
            contents.insert(name, fs::read(entry.path()).unwrap());
        }
    }
    contents
}

async fn executable_config(directory: &Path, data: &Path) -> PathBuf {
    let users = directory.join("users.json");
    write_users(&users, &[(TEST_USERNAME, TEST_PASSWORD)]).await;
    let config = directory.join("server.json");
    fs::write(
        &config,
        json!({
            "listen_address": "127.0.0.1:0",
            "public_url": "http://127.0.0.1:8080",
            "users_file": users,
            "data_directory": data,
        })
        .to_string(),
    )
    .unwrap();
    config
}

async fn start_executable(config: &Path) -> Child {
    let mut child = Command::new("/bin/sh")
        .args(["-c", "umask 022; exec \"$0\" --config \"$1\""])
        .arg(env!("CARGO_BIN_EXE_lorebayd"))
        .arg(config)
        .env_clear()
        .env("TOKIO_WORKER_THREADS", "2")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let address = timeout(STARTUP_TIMEOUT, async {
        let mut lines = BufReader::new(child.stderr.as_mut().unwrap()).lines();
        loop {
            let line = lines
                .next_line()
                .await
                .unwrap()
                .expect("server stopped before listening");
            let entry: Value = serde_json::from_str(&line).unwrap();
            if let Some(address) = entry["fields"]["listen_address"].as_str() {
                break address.to_owned();
            }
        }
    })
    .await
    .unwrap();
    let response = Client::builder()
        .no_proxy()
        .timeout(TEST_TIMEOUT)
        .build()
        .unwrap()
        .get(format!("http://{address}/readyz"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    child
}

async fn stop_executable(child: &mut Child) {
    let status = Command::new("/bin/kill")
        .arg("-TERM")
        .arg(child.id().unwrap().to_string())
        .status()
        .await
        .unwrap();
    assert!(status.success());
    assert!(
        timeout(TEST_TIMEOUT, child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

async fn connect_without_foreign_keys(path: &Path) -> DatabaseConnection {
    let filename = path.to_path_buf();
    let mut options = ConnectOptions::new("sqlite://");
    options.map_sqlx_sqlite_opts(move |sqlite| sqlite.filename(&filename).foreign_keys(false));
    Database::connect(options).await.unwrap()
}
