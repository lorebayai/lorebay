use std::fs;
use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use argon2::password_hash::PasswordHasher;
use argon2::{Algorithm, Argon2, Params, Version};
use lorebayd::{Config, hash_password, run};
use reqwest::{Client, Method, Response, StatusCode};
use sea_orm::sea_query::{Alias, Expr, Query};
use sea_orm::{ConnectionTrait, Database};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use typed_builder::TypedBuilder;
use url::Url;

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
    pub directory: TempDir,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<()>>>,
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
        let parameters = Params::new(65_536, 3, 4, Some(32)).unwrap();
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

    async fn start_with(
        directory: TempDir,
        users: &[(&str, &str)],
        configure: impl FnOnce(&mut Config),
    ) -> Self {
        let users_file = directory.path().join("users.json");
        let users: Vec<_> = users.iter().map(|(username, password)| {
            json!({"username": username, "password_hash": hash_password(password).unwrap()})
        }).collect();
        fs::write(&users_file, serde_json::to_vec(&users).unwrap()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let mut config = Config::builder()
            .listen_address(address)
            .data_directory(directory.path().join("data"))
            .users_file(Some(users_file))
            .public_url(format!("http://{address}"))
            .build();
        configure(&mut config);
        let (shutdown, stopped) = oneshot::channel();
        let task = tokio::spawn(run(config, listener, async {
            let _ = stopped.await;
        }));
        let server = Self {
            client: Client::builder()
                .timeout(TEST_TIMEOUT)
                .no_proxy()
                .build()
                .unwrap(),
            address,
            directory,
            shutdown: Some(shutdown),
            task: Some(task),
        };
        server.wait_for_status("/readyz", StatusCode::OK).await;
        server
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
        let response = self.log_in(TEST_USERNAME, TEST_PASSWORD).await;
        assert_eq!(response.status(), StatusCode::OK);
        response.json::<Value>().await.unwrap()["data"].clone()
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
        self.shutdown.take().unwrap().send(()).unwrap();
    }

    pub async fn wait_for_status(&self, path: &str, expected: StatusCode) {
        timeout(TEST_TIMEOUT, async {
            loop {
                if let Ok(response) = self
                    .client
                    .get(format!("http://{}{path}", self.address))
                    .send()
                    .await
                    && response.status() == expected
                {
                    break;
                }
                assert!(
                    !self.task.as_ref().unwrap().is_finished(),
                    "server exited before {expected}"
                );
                sleep(POLL_INTERVAL).await;
            }
        })
        .await
        .unwrap();
    }

    pub async fn finish_shutdown(&mut self) {
        timeout(TEST_TIMEOUT, self.task.take().unwrap())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    pub async fn wait_for_listener_to_close(&self) {
        timeout(TEST_TIMEOUT, async {
            while TcpStream::connect(self.address).await.is_ok() {
                sleep(POLL_INTERVAL).await;
            }
        })
        .await
        .unwrap();
        assert!(!self.task.as_ref().unwrap().is_finished());
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

const POLL_INTERVAL: Duration = Duration::from_millis(10);
mod oauth;
