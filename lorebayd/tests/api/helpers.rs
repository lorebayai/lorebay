use std::fs;
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;

use argon2::password_hash::PasswordHasher;
use argon2::{Algorithm, Argon2, Params, Version};
use lorebayd::Config;
use reqwest::{Client, Method, Response, StatusCode};
use sea_orm::sea_query::{Alias, Expr, Query};
use sea_orm::{ConnectionTrait, Database};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Instant, sleep, timeout};
use typed_builder::TypedBuilder;
use url::Url;

use server::ServerTask;

pub use oauth::*;

pub const TEST_TIMEOUT: Duration = Duration::from_secs(15);
pub const TEST_USERNAME: &str = "alice";
pub const TEST_PASSWORD: &str = "correct horse battery staple";

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct Settings {
    #[builder(default = 900)]
    pub access_seconds: u64,
    #[builder(default = 2_592_000)]
    pub refresh_seconds: u64,
    #[builder(default = 10)]
    pub rate_limit: u32,
    #[builder(default = 60)]
    pub rate_window_seconds: u64,
}

pub struct Server {
    pub client: Client,
    pub address: SocketAddr,
    pub public_url: String,
    task: ServerTask,
    pub directory: TempDir,
}

impl Default for Settings {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl Server {
    pub async fn start() -> Self {
        Self::restart(TempDir::new().unwrap()).await
    }

    pub async fn restart(directory: TempDir) -> Self {
        Self::restart_with_users(directory, &[(TEST_USERNAME, TEST_PASSWORD)]).await
    }

    pub async fn restart_configured(
        directory: TempDir,
        configure: impl FnOnce(&mut Config),
    ) -> Self {
        Self::start_with(directory, &[(TEST_USERNAME, TEST_PASSWORD)], configure).await
    }

    pub async fn restart_with_users(directory: TempDir, users: &[(&str, &str)]) -> Self {
        Self::start_with(directory, users, |_| {}).await
    }

    pub async fn start_with_settings(settings: Settings) -> Self {
        Self::configured(|config| {
            config.access_token_lifetime_seconds = settings.access_seconds;
            config.refresh_token_lifetime_seconds = settings.refresh_seconds;
            config.login_rate_limit = settings.rate_limit;
            config.login_rate_window_seconds = settings.rate_window_seconds;
        })
        .await
    }

    pub async fn start_with_nondefault_hash() -> Self {
        let parameters = Params::new(4_096, 1, 1, Some(32)).unwrap();
        let hash = Argon2::new(Algorithm::Argon2id, Version::V0x13, parameters)
            .hash_password(TEST_PASSWORD.as_bytes())
            .unwrap()
            .to_string();
        Self::configured(|config| {
            let users = json!([{ "username": TEST_USERNAME, "password_hash": hash }]);
            fs::write(
                config.users_file.as_ref().unwrap(),
                serde_json::to_vec(&users).unwrap(),
            )
            .unwrap();
        })
        .await
    }

    pub async fn configured(configure: impl FnOnce(&mut Config)) -> Self {
        Self::start_with(
            TempDir::new().unwrap(),
            &[(TEST_USERNAME, TEST_PASSWORD)],
            configure,
        )
        .await
    }

    pub async fn get(&self, path: &str) -> Response {
        self.client
            .get(format!("http://{}{path}", self.address))
            .send()
            .await
            .unwrap()
    }

    pub async fn json(
        &self,
        method: Method,
        path: &str,
        body: Value,
        token: Option<&str>,
    ) -> Response {
        let mut request = self
            .client
            .request(method, format!("http://{}{path}", self.address))
            .json(&body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        request.send().await.unwrap()
    }

    pub async fn log_in(&self, username: &str, password: &str) -> Response {
        self.json(
            Method::POST,
            "/api/v1/login-session",
            json!({"username": username, "password": password}),
            None,
        )
        .await
    }

    pub async fn tokens(&self) -> Value {
        login(self, TEST_USERNAME).await
    }

    pub async fn refresh(&self, token: &str) -> Response {
        self.json(
            Method::POST,
            "/api/v1/login-session/refresh",
            json!({"refresh_token": token}),
            None,
        )
        .await
    }

    pub async fn log_out(&self, token: Option<&str>) -> Response {
        self.json(Method::DELETE, "/api/v1/login-session", Value::Null, token)
            .await
    }

    pub async fn expire_login_sessions(&self) {
        let path = self.directory.path().join("data/lorebay.sqlite3");
        let database = Database::connect(format!("sqlite://{}", path.display()))
            .await
            .unwrap();
        let statement = Query::update()
            .table(Alias::new("login_session"))
            .value(Alias::new("expires_at"), Expr::value(0_i64))
            .to_owned();
        database.execute(&statement).await.unwrap();
        database.close().await.unwrap();
    }

    pub fn begin_shutdown(&mut self) {
        self.task.begin_shutdown();
    }

    pub async fn wait_for_status(&mut self, path: &str, expected: StatusCode) {
        self.wait_for_status_with_budget(path, expected, TEST_TIMEOUT)
            .await;
    }

    pub async fn finish_shutdown(&mut self) {
        self.task.finish_shutdown(TEST_TIMEOUT).await;
    }

    pub async fn wait_for_listener_to_close(&mut self) {
        timeout(TEST_TIMEOUT, async {
            while TcpStream::connect(self.address).await.is_ok() {
                sleep(POLL_INTERVAL).await;
            }
        })
        .await
        .unwrap();
        self.task.assert_running();
    }

    pub async fn stop(mut self) -> TempDir {
        self.begin_shutdown();
        self.finish_shutdown().await;
        self.directory
    }

    pub async fn begin_upload(&self) -> TcpStream {
        let mut stream = TcpStream::connect(self.address).await.unwrap();
        stream.write_all(b"POST /api/v1/unknown HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nExpect: 100-continue\r\nConnection: close\r\n\r\n").await.unwrap();
        let mut interim = [0; 25];
        timeout(TEST_TIMEOUT, stream.read_exact(&mut interim))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&interim, b"HTTP/1.1 100 Continue\r\n\r\n");
        stream
    }

    async fn start_with(
        directory: TempDir,
        users: &[(&str, &str)],
        configure: impl FnOnce(&mut Config),
    ) -> Self {
        let users_file = directory.path().join("users.json");
        fixtures::write_users(&users_file, users).await;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let public_url_file = directory.path().join("public-url");
        let public_url = if public_url_file.exists() {
            fs::read_to_string(&public_url_file).unwrap()
        } else {
            format!("http://{address}")
        };
        let mut config = Config::builder()
            .listen_address(address)
            .data_directory(directory.path().join("data"))
            .users_file(Some(users_file))
            .public_url(public_url)
            .build();
        configure(&mut config);
        fs::write(public_url_file, &config.public_url).unwrap();
        let public_url = config.public_url.clone();
        let task = ServerTask::start(config, listener);
        let mut server = Self {
            client: Client::builder()
                .timeout(TEST_TIMEOUT)
                .no_proxy()
                .build()
                .unwrap(),
            address,
            public_url,
            directory,
            task,
        };
        server
            .wait_for_status_with_budget("/readyz", StatusCode::OK, STARTUP_TIMEOUT)
            .await;
        server
    }

    async fn wait_for_status_with_budget(
        &mut self,
        path: &str,
        expected: StatusCode,
        budget: Duration,
    ) {
        let deadline = Instant::now() + budget;
        loop {
            self.task.assert_running();
            let response = self
                .client
                .get(format!("http://{}{path}", self.address))
                .timeout(PROBE_TIMEOUT)
                .send()
                .await;
            self.task.assert_running();
            if response
                .as_ref()
                .is_ok_and(|response| response.status() == expected)
            {
                return;
            }
            if Instant::now() >= deadline {
                self.task.fail(&format!(
                    "server did not return {expected} for {path} within {budget:?}; last probe: {response:?}"
                ));
            }
            sleep(POLL_INTERVAL).await;
        }
    }
}

pub async fn list_response(
    path: &str,
    server: &Server,
    tokens: &Value,
    query: &[(&str, &str)],
) -> Response {
    let mut url = Url::parse(&format!("http://{}{path}", server.address)).unwrap();
    url.query_pairs_mut().extend_pairs(query.iter().copied());
    server
        .client
        .get(url)
        .bearer_auth(tokens["access_token"].as_str().unwrap())
        .send()
        .await
        .unwrap()
}

pub async fn assert_problem(response: Response, status: StatusCode, code: &str) -> Value {
    assert_eq!(response.status(), status);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    if status == StatusCode::UNAUTHORIZED {
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
    }
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(body["status"], status.as_u16());
    assert_eq!(body["code"], code);
    body
}

pub async fn login(server: &Server, username: &str) -> Value {
    let response = server.log_in(username, TEST_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json::<Value>().await.unwrap()["data"].clone()
}

pub async fn request(
    server: &Server,
    tokens: &Value,
    method: Method,
    path: &str,
    body: Value,
) -> Response {
    server
        .json(
            method,
            path,
            body,
            Some(tokens["access_token"].as_str().unwrap()),
        )
        .await
}

pub async fn two_people() -> Server {
    Server::restart_with_users(
        TempDir::new().unwrap(),
        &[(TEST_USERNAME, TEST_PASSWORD), ("bob", TEST_PASSWORD)],
    )
    .await
}

pub async fn create_space(server: &Server, tokens: &Value, name: &str, summary: &str) -> Value {
    let response = request(
        server,
        tokens,
        Method::POST,
        "/api/v1/spaces",
        json!({"name": name, "summary": summary}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = response.json::<Value>().await.unwrap();
    assert_eq!(body["meta"], json!({}));
    body["data"].clone()
}

pub async fn authorize_for_client(
    server: &Server,
    client: &str,
    name: &str,
    person: &Value,
) -> AuthorizedAgent {
    let code = server
        .approve_code_for(client, Some(name), person["access_token"].as_str().unwrap())
        .await;
    let response = server.exchange_code(client, &code).await;
    assert_eq!(response.status(), StatusCode::OK);
    AuthorizedAgent::builder()
        .client_id(client)
        .tokens(response.json::<Value>().await.unwrap())
        .build()
}

pub async fn authorize_for(server: &Server, tokens: &Value, name: &str) -> Value {
    let client = server.register_client(name).await;
    authorize_for_client(server, &client, name, tokens)
        .await
        .tokens
}

pub async fn list_page(
    server: &Server,
    tokens: &Value,
    path: &str,
    query: &[(&str, &str)],
) -> Value {
    let response = list_response(path, server, tokens, query).await;
    assert_eq!(response.status(), StatusCode::OK);
    response.json().await.unwrap()
}

pub async fn list(server: &Server, tokens: &Value, path: &str) -> Value {
    list_page(server, tokens, path, &[]).await["data"].clone()
}

pub async fn walk(
    server: &Server,
    tokens: &Value,
    path: &str,
    size: &str,
    max_items: usize,
) -> Vec<Value> {
    let mut rows = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut query = vec![("page_size", size)];
        if let Some(cursor) = &cursor {
            query.push(("cursor", cursor));
        }
        let page = list_page(server, tokens, path, &query).await;
        let items = page["data"].as_array().unwrap();
        assert!(items.len() <= size.parse::<usize>().unwrap());
        rows.extend(items.iter().cloned());
        assert!(rows.len() <= max_items);
        match page["meta"]["next_cursor"].as_str() {
            Some(next) => {
                assert_eq!(items.len(), size.parse::<usize>().unwrap());
                assert_ne!(cursor.as_deref(), Some(next));
                cursor = Some(next.to_owned());
            }
            None => break,
        }
    }
    rows
}

const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const PROBE_TIMEOUT: Duration = Duration::from_millis(250);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
mod fixtures;
mod oauth;
mod server;
