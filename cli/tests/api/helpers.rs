use std::fs::{self, File};
use std::path::PathBuf;
use std::process::{Child as ServerChild, Command as ServerCommand, Output, Stdio};
use std::time::Duration;

use reqwest::redirect::Policy;
use reqwest::{Client, Method, StatusCode};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::process::{Child, ChildStderr, Command};
use tokio::time::{Instant, sleep, timeout};
use url::Url;

mod team;
pub use team::*;

pub const SECOND_USERNAME: &str = "teammate /?#+%";

pub const CREDENTIALS_FILE: &str = "credentials.json";
pub const LOGIN_SESSION_PATH: &str = "/api/v1/login-session";
pub const WAIT: Duration = Duration::from_secs(30);
pub const HTTP_WAIT: Duration = Duration::from_secs(10);
pub const PASSWORD: &str = "scenario password";
pub const CLIENT_ID: &str = "lorebay-cli";
pub const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
pub const CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
pub const REDIRECT: &str = "http://127.0.0.1:46137/callback";

pub struct Server {
    process: Daemon,
    pub client: Client,
    pub directory: TempDir,
    pub url: String,
    pub username: String,
}

pub struct SigningIn {
    pub link: Url,
    child: Child,
    stderr: BufReader<ChildStderr>,
    prefix: Vec<u8>,
}

impl Server {
    pub async fn start() -> Self {
        Self::configured(json!({})).await
    }

    pub async fn configured(settings: Value) -> Self {
        let directory = TempDir::new().unwrap();
        let username = directory
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let users = directory.path().join("users.json");
        write_users(&users, &username).await;
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .timeout(HTTP_WAIT)
            .build()
            .unwrap();
        for _ in 0..10 {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            drop(listener);
            let url = format!("http://{address}");
            let mut config = json!({"listen_address": address.to_string(), "public_url": url,
                "users_file": users, "data_directory": directory.path().join("data"),
                "login_rate_limit": 1000, "registration_rate_limit": 1000});
            config
                .as_object_mut()
                .unwrap()
                .extend(settings.as_object().unwrap().clone());
            let path = directory.path().join("server.json");
            fs::write(&path, config.to_string()).unwrap();
            let mut process = Daemon(
                ServerCommand::new(server_binary())
                    .args(["--config"])
                    .arg(path)
                    .env_clear()
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(File::create(directory.path().join("server.log")).unwrap())
                    .spawn()
                    .unwrap(),
            );
            if ready(&client, &url, &mut process, &username).await {
                return Self {
                    client,
                    directory,
                    url,
                    username,
                    process,
                };
            }
        }
        panic!("lorebayd could not start on a free port");
    }

    pub fn home(&self) -> PathBuf {
        self.directory.path().join("cli")
    }

    pub fn credentials(&self) -> Value {
        serde_json::from_slice(&fs::read(self.home().join(CREDENTIALS_FILE)).unwrap()).unwrap()
    }

    pub fn cli(&self, arguments: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lorebay"));
        command
            .args(arguments)
            .env_clear()
            .env("HOME", self.directory.path().join("home"))
            .env("LOREBAY_HOME", self.home())
            .current_dir(self.directory.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command
    }

    pub async fn run(&self, arguments: &[&str]) -> Output {
        timeout(WAIT, self.cli(arguments).output())
            .await
            .unwrap()
            .unwrap()
    }

    pub async fn signing_in(&self, arguments: &[&str]) -> SigningIn {
        let child = self.cli(arguments).spawn().unwrap();
        SigningIn::start(child).await
    }

    pub async fn login_as(&self, username: &str) -> Output {
        let mut command = self.cli(&["login", &self.url, "--no-browser", "--output", "json"]);
        command.env("LOREBAY_HOME", self.directory.path().join(username));
        let login = SigningIn::start(command.spawn().unwrap()).await;
        self.decide_as(&login.link, "approve", username).await;
        let output = login.finish().await;
        assert_success(&output);
        output
    }

    pub async fn run_as(&self, username: &str, arguments: &[&str]) -> Output {
        timeout(
            WAIT,
            self.cli(arguments)
                .env("LOREBAY_HOME", self.directory.path().join(username))
                .output(),
        )
        .await
        .unwrap()
        .unwrap()
    }

    pub async fn login(&self) -> Output {
        let login = self
            .signing_in(&["login", &self.url, "--no-browser", "--output", "json"])
            .await;
        self.decide(&login.link, "approve").await;
        let output = login.finish().await;
        assert_success(&output);
        output
    }

    pub async fn json(
        &self,
        method: Method,
        path: &str,
        body: Value,
        token: Option<&str>,
    ) -> Value {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.url))
            .json(&body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.unwrap();
        assert!(response.status().is_success(), "{}", response.status());
        response.json().await.unwrap()
    }

    pub async fn person(&self) -> Value {
        self.person_as(&self.username).await
    }

    async fn person_as(&self, username: &str) -> Value {
        self.json(
            Method::POST,
            LOGIN_SESSION_PATH,
            json!({"username": username, "password": PASSWORD}),
            None,
        )
        .await["data"]
            .clone()
    }

    pub async fn decide(&self, link: &Url, decision: &str) {
        self.decide_as(link, decision, &self.username).await;
    }

    async fn decide_as(&self, link: &Url, decision: &str, username: &str) {
        assert_eq!(
            self.client.get(link.clone()).send().await.unwrap().status(),
            StatusCode::OK
        );
        let person = self.person_as(username).await;
        let mut request = parameters(link);
        self.json(
            Method::POST,
            "/api/v1/authorization-request",
            request.clone(),
            person["access_token"].as_str(),
        )
        .await;
        request["decision"] = json!(decision);
        let body = self
            .json(
                Method::POST,
                "/api/v1/authorization-decision",
                request,
                person["access_token"].as_str(),
            )
            .await;
        self.json(
            Method::DELETE,
            LOGIN_SESSION_PATH,
            Value::Null,
            person["access_token"].as_str(),
        )
        .await;
        let redirect = body["data"]["redirect_url"].as_str().unwrap();
        assert_eq!(
            self.client.get(redirect).send().await.unwrap().status(),
            StatusCode::OK
        );
    }

    pub async fn agent(&self, name: &str) -> Value {
        let client = self
            .json(
                Method::POST,
                "/oauth/register",
                json!({"client_name": "Scenario client",
            "redirect_uris": [REDIRECT], "token_endpoint_auth_method": "none"}),
                None,
            )
            .await;
        let person = self.person().await;
        let body = self
            .json(
                Method::POST,
                "/api/v1/authorization-decision",
                json!({
            "client_id": client["client_id"], "redirect_uri": REDIRECT, "response_type": "code",
            "code_challenge": CHALLENGE, "code_challenge_method": "S256", "decision": "approve",
            "agent_name": name}),
                person["access_token"].as_str(),
            )
            .await;
        let redirect = Url::parse(body["data"]["redirect_url"].as_str().unwrap()).unwrap();
        let code = query(&redirect, "code");
        self.token(&[
            ("grant_type", "authorization_code"),
            ("client_id", client["client_id"].as_str().unwrap()),
            ("redirect_uri", REDIRECT),
            ("code", &code),
            ("code_verifier", VERIFIER),
        ])
        .await
    }

    pub async fn token(&self, form: &[(&str, &str)]) -> Value {
        let response = self
            .client
            .post(format!("{}/oauth/token", self.url))
            .form(form)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response.json().await.unwrap()
    }

    pub fn stop(&mut self) {
        self.process.0.kill().unwrap();
        self.process.0.wait().unwrap();
    }

    pub async fn set_access_lifetime(&mut self, seconds: u64) {
        self.stop();
        let path = self.directory.path().join("server.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        config["access_token_lifetime_seconds"] = json!(seconds);
        fs::write(&path, config.to_string()).unwrap();
        self.process = Daemon(
            ServerCommand::new(server_binary())
                .arg("--config")
                .arg(path)
                .env_clear()
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        assert!(ready(&self.client, &self.url, &mut self.process, &self.username).await);
    }
}

impl SigningIn {
    pub async fn start(mut child: Child) -> Self {
        let mut stderr = BufReader::new(child.stderr.take().unwrap());
        let mut prefix = String::new();
        timeout(WAIT, stderr.read_line(&mut prefix))
            .await
            .unwrap()
            .unwrap();
        let link = Url::parse(
            prefix
                .trim()
                .strip_prefix("Open this URL to sign in: ")
                .unwrap(),
        )
        .unwrap();
        Self {
            link,
            child,
            stderr,
            prefix: prefix.into_bytes(),
        }
    }

    pub async fn finish(mut self) -> Output {
        timeout(WAIT, async {
            let mut rest = Vec::new();
            let (output, read) = tokio::join!(
                self.child.wait_with_output(),
                self.stderr.read_to_end(&mut rest)
            );
            read.unwrap();
            let mut output = output.unwrap();
            self.prefix.extend(rest);
            output.stderr = self.prefix;
            output
        })
        .await
        .unwrap()
    }
}

pub fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "exit {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn body(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap()
}

pub fn query(url: &Url, key: &str) -> String {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .unwrap()
        .1
        .into_owned()
}

pub fn parameters(link: &Url) -> Value {
    Value::Object(
        link.query_pairs()
            .map(|(name, value)| (name.into_owned(), json!(value)))
            .collect(),
    )
}

async fn write_users(path: &PathBuf, username: &str) {
    let hash = password_hash().await;
    fs::write(
        path,
        json!([
            {"username": username, "password_hash": hash},
            {"username": SECOND_USERNAME, "password_hash": hash}
        ])
        .to_string(),
    )
    .unwrap();
}

async fn password_hash() -> String {
    let mut child = Command::new(server_binary())
        .arg("hash-password")
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(PASSWORD.as_bytes())
        .await
        .unwrap();
    let output = timeout(WAIT, child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

async fn ready(client: &Client, url: &str, process: &mut Daemon, username: &str) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if process.0.try_wait().unwrap().is_some() {
            return false;
        }
        if let Ok(response) = client.get(format!("{url}/readyz")).send().await
            && response.status() == StatusCode::OK
            && owns_server(client, url, username).await
            && process.0.try_wait().unwrap().is_none()
        {
            return true;
        }
        sleep(Duration::from_millis(25)).await;
    }
    false
}

async fn owns_server(client: &Client, url: &str, username: &str) -> bool {
    let Ok(response) = client
        .post(format!("{url}/api/v1/login-session"))
        .json(&json!({"username": username, "password": PASSWORD}))
        .send()
        .await
    else {
        return false;
    };
    if response.status() != StatusCode::OK {
        return false;
    }
    let Ok(body) = response.json::<Value>().await else {
        return false;
    };
    let Some(token) = body["data"]["access_token"].as_str() else {
        return false;
    };
    client
        .delete(format!("{url}/api/v1/login-session"))
        .bearer_auth(token)
        .send()
        .await
        .is_ok_and(|response| response.status() == StatusCode::OK)
}

fn server_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_lorebay")).with_file_name("lorebayd")
}

struct Daemon(ServerChild);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
