use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{sleep, timeout};
use typed_builder::TypedBuilder;

use crate::helpers::CREDENTIALS_FILE;
use crate::helpers::{Server, SigningIn, WAIT, assert_success, body};

#[tokio::test]
async fn agent_revoke_repeats_json_body_when_session_refreshes_after_unauthorized() {
    let mut proxy = Proxy::configured(json!({"access_token_lifetime_seconds": 2})).await;
    proxy.login().await;
    proxy.server.set_access_lifetime(900).await;
    proxy.server.agent("Body retry agent").await;
    let mut saved = proxy.server.credentials();
    let previous_refresh = saved["refresh_token"].clone();
    saved["expires_at"] = json!(u64::MAX);
    fs::write(
        proxy.server.home().join(CREDENTIALS_FILE),
        saved.to_string(),
    )
    .unwrap();
    sleep(Duration::from_secs(3)).await;
    let output = proxy
        .server
        .run(&[
            "agent",
            "revoke",
            "Body retry agent",
            "--yes",
            "--output",
            "json",
        ])
        .await;
    assert_success(&output);
    assert_eq!(body(&output), json!({"data": null, "meta": {}}));
    let requests = proxy.requests.lock().await;
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        assert_eq!(request["body"], "{}");
        assert_eq!(request["content_type"], "application/json");
    }
    assert_eq!(requests[0]["status"], 401);
    assert_eq!(requests[1]["status"], 200);
    drop(requests);
    assert_eq!(proxy.refreshes.load(Ordering::SeqCst), 1);
    assert!(proxy.server.credentials()["refresh_token"] != previous_refresh);
    let remaining = proxy
        .server
        .run(&["agent", "list", "--output", "json"])
        .await;
    assert_success(&remaining);
    assert_eq!(body(&remaining)["data"], json!([]));
}

#[tokio::test]
async fn agent_list_requires_new_login_when_refresh_response_was_lost() {
    let proxy = Proxy::start().await;
    proxy.login().await;
    let mut saved = proxy.server.credentials();
    saved["expires_at"] = json!(0);
    fs::write(
        proxy.server.home().join(CREDENTIALS_FILE),
        saved.to_string(),
    )
    .unwrap();
    let before = fs::read(proxy.server.home().join(CREDENTIALS_FILE)).unwrap();
    proxy.fault.store(LOSE_REFRESH, Ordering::SeqCst);
    let first = proxy
        .server
        .run(&["agent", "list", "--output", "json"])
        .await;
    assert_eq!(first.status.code(), Some(1));
    assert_eq!(proxy.refreshes.load(Ordering::SeqCst), 1);
    assert!(before == fs::read(proxy.server.home().join(CREDENTIALS_FILE)).unwrap());
    let retry = proxy
        .server
        .run(&["agent", "list", "--output", "json"])
        .await;
    assert_eq!(retry.status.code(), Some(4));
    assert_eq!(proxy.refreshes.load(Ordering::SeqCst), 2);
    assert!(!proxy.server.home().join(CREDENTIALS_FILE).exists());
    let error: Value = serde_json::from_slice(&retry.stderr).unwrap();
    assert_eq!(error["code"], "invalid_grant");
}

#[tokio::test]
async fn agent_list_refuses_redirect_to_another_server() {
    let proxy = Proxy::start().await;
    proxy.login().await;
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = trap.local_addr().unwrap();
    let saved = proxy.server.credentials();
    fs::write(
        proxy.server.directory.path().join("redirect"),
        format!("http://{address}/trap"),
    )
    .unwrap();
    proxy.fault.store(REDIRECT_CALL, Ordering::SeqCst);
    let output = proxy
        .server
        .run(&["agent", "list", "--output", "json"])
        .await;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(
        timeout(Duration::from_millis(100), trap.accept())
            .await
            .is_err()
    );
    for key in ["access_token", "refresh_token"] {
        assert!(!String::from_utf8_lossy(&output.stderr).contains(saved[key].as_str().unwrap()));
    }
    assert_success(&proxy.server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn login_refuses_metadata_endpoint_on_another_server() {
    let proxy = Proxy::start().await;
    proxy.fault.store(CHANGE_METADATA, Ordering::SeqCst);
    let output = proxy
        .server
        .run(&["login", &proxy.url, "--no-browser", "--output", "json"])
        .await;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("Open this URL"));
    assert!(!proxy.server.home().join(CREDENTIALS_FILE).exists());
}

const CHANGE_METADATA: u8 = 1;
const LOSE_REFRESH: u8 = 2;
const REDIRECT_CALL: u8 = 3;

struct Proxy {
    server: Server,
    url: String,
    fault: Arc<AtomicU8>,
    refreshes: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct ProxyState {
    upstream: String,
    fault: Arc<AtomicU8>,
    refreshes: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Value>>>,
    redirect_path: PathBuf,
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Proxy {
    async fn start() -> Self {
        Self::configured(json!({})).await
    }

    #[expect(
        clippy::unwrap_used,
        reason = "The scenario fails immediately when its proxy cannot start."
    )]
    async fn configured(mut settings: Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        settings["public_url"] = json!(url);
        let server = Server::configured(settings).await;
        let fault = Arc::new(AtomicU8::new(0));
        let refreshes = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ProxyState::builder()
            .upstream(server.url.clone())
            .fault(fault.clone())
            .refreshes(refreshes.clone())
            .requests(requests.clone())
            .redirect_path(server.directory.path().join("redirect"))
            .build();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    Ok((socket, _)) = listener.accept() => {
                        let state = state.clone();
                        connections.spawn(async move { let _ = timeout(WAIT, forward(socket, state)).await; });
                    },
                    Some(_) = connections.join_next(), if !connections.is_empty() => {},
                }
            }
        });
        Self {
            server,
            url,
            fault,
            refreshes,
            requests,
            task,
        }
    }

    #[expect(
        clippy::unwrap_used,
        reason = "The scenario fails immediately when its CLI cannot start."
    )]
    async fn login(&self) {
        let child = self
            .server
            .cli(&["login", &self.url, "--no-browser"])
            .spawn()
            .unwrap();
        let login = SigningIn::start(child).await;
        self.server.decide(&login.link, "approve").await;
        assert_success(&login.finish().await);
    }
}

async fn forward(mut socket: TcpStream, state: ProxyState) -> io::Result<()> {
    let ProxyState {
        upstream,
        fault,
        refreshes,
        requests,
        redirect_path,
    } = state;
    let (mut request, headers, body) = read_request(&mut socket).await?;
    let refresh =
        headers.starts_with("POST /oauth/token ") && body.starts_with(b"grant_type=refresh_token");
    if refresh {
        refreshes.fetch_add(1, Ordering::SeqCst);
    }
    let mut target = TcpStream::connect(upstream.trim_start_matches("http://")).await?;
    request.truncate(request.len() - 2);
    request.extend_from_slice(b"Connection: close\r\n\r\n");
    request.extend_from_slice(&body);
    target.write_all(&request).await?;
    let mut response = Vec::new();
    target.read_to_end(&mut response).await?;
    record_request(&requests, &headers, &body, &response).await;
    if refresh
        && fault
            .compare_exchange(LOSE_REFRESH, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        return Ok(());
    }
    if headers.starts_with("GET /api/v1/agents")
        && fault
            .compare_exchange(REDIRECT_CALL, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        let location = fs::read_to_string(redirect_path)?;
        response = format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes();
    }
    if headers.starts_with("GET /.well-known/")
        && fault
            .compare_exchange(CHANGE_METADATA, 0, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    {
        let start = response
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap_or(response.len());
        let mut metadata: Value =
            serde_json::from_slice(&response[start.saturating_add(4).min(response.len())..])?;
        metadata["token_endpoint"] = json!("http://127.0.0.1:1/oauth/token");
        let body = metadata.to_string();
        response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes();
    }
    socket.write_all(&response).await
}

async fn record_request(requests: &Mutex<Vec<Value>>, headers: &str, body: &[u8], response: &[u8]) {
    if headers.starts_with("DELETE /api/v1/agents/") {
        let status = String::from_utf8_lossy(response)
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(0);
        let content_type = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-type: "))
            .unwrap_or_default();
        requests
            .lock()
            .await
            .push(json!({"body": String::from_utf8_lossy(body),
            "content_type": content_type, "status": status}));
    }
}

async fn read_request(socket: &mut TcpStream) -> io::Result<(Vec<u8>, String, Vec<u8>)> {
    let mut request = Vec::new();
    let mut byte = [0];
    while request.len() < 65536 && !request.ends_with(b"\r\n\r\n") {
        if socket.read(&mut byte).await? == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        request.push(byte[0]);
    }
    let headers = String::from_utf8_lossy(&request).into_owned();
    let length = headers
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length: ")
                .and_then(|value| value.parse::<usize>().ok())
        })
        .unwrap_or(0);
    let mut body = vec![0; length.min(65536)];
    socket.read_exact(&mut body).await?;
    Ok((request, headers, body))
}
