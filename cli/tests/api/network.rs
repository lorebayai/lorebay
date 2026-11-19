use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use httpdate::fmt_http_date;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{Instant, sleep, timeout};
use typed_builder::TypedBuilder;

use crate::helpers::CREDENTIALS_FILE;
use crate::helpers::{
    DeviceSigningIn, Server, SigningIn, WAIT, assert_success, assert_usage, body, create,
    path_commands,
};

#[tokio::test]
async fn path_arguments_fail_without_request_when_segment_is_empty_or_dot() {
    let proxy = Proxy::start().await;
    proxy.login().await;
    let space = create(&proxy.server, "Keep team", "Keep summary").await;
    proxy.server.agent("Keep agent").await;
    let path = proxy.server.home().join(CREDENTIALS_FILE);
    let original = fs::read(&path).unwrap();
    let mut saved = proxy.server.credentials();
    saved["expires_at"] = json!(0);
    fs::write(&path, saved.to_string()).unwrap();
    let unchanged = fs::read(&path).unwrap();
    let calls = proxy.calls.load(Ordering::SeqCst);
    for mode in ["human", "json"] {
        for mut arguments in ["..", ".", ""].into_iter().flat_map(path_commands) {
            arguments.extend(["--output", mode]);
            let output = proxy.server.run(&arguments).await;
            assert_eq!(output.status.code(), Some(2), "{arguments:?}");
            assert!(output.stdout.is_empty());
            assert_eq!(String::from_utf8_lossy(&output.stderr).lines().count(), 1);
            if mode == "json" {
                assert_usage(&output);
            }
            assert_eq!(proxy.calls.load(Ordering::SeqCst), calls, "{arguments:?}");
            assert_eq!(proxy.refreshes.load(Ordering::SeqCst), 0);
        }
    }
    assert_eq!(fs::read(&path).unwrap(), unchanged);
    fs::write(&path, original).unwrap();
    let shown = proxy
        .server
        .run(&["space", "show", &space, "--output", "json"])
        .await;
    assert_success(&shown);
    assert_eq!(body(&shown)["data"]["name"], "Keep team");
    assert_eq!(body(&shown)["data"]["summary"], "Keep summary");
    let agents = proxy
        .server
        .run(&["agent", "list", "--output", "json"])
        .await;
    assert_success(&agents);
    assert_eq!(body(&agents)["data"][0]["name"], "Keep agent");
}

#[tokio::test]
async fn space_create_repeats_json_body_when_session_refreshes_after_unauthorized() {
    let mut proxy = Proxy::configured(json!({"access_token_lifetime_seconds": 2})).await;
    proxy.login().await;
    proxy.server.set_access_lifetime(900).await;
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
            "space",
            "create",
            "--name",
            "Retry team",
            "--summary",
            "Real body",
            "--output",
            "json",
        ])
        .await;
    assert_success(&output);
    let expected = json!({"data": {"key": "SPACE-1", "name": "Retry team", "summary": "Real body", "role": "owner", "caller_person": proxy.server.username}, "meta": {}});
    assert_eq!(body(&output), expected);
    let requests = proxy.requests.lock().await;
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        assert_eq!(
            serde_json::from_str::<Value>(request["body"].as_str().unwrap()).unwrap(),
            json!({"name": "Retry team", "summary": "Real body"})
        );
        assert_eq!(request["content_type"], "application/json");
    }
    assert_eq!(requests[0]["status"], 401);
    assert_eq!(requests[1]["status"], 201);
    drop(requests);
    assert_eq!(proxy.refreshes.load(Ordering::SeqCst), 1);
    assert!(proxy.server.credentials()["refresh_token"] != previous_refresh);
    let shown = proxy
        .server
        .run(&["space", "show", "SPACE-1", "--output", "json"])
        .await;
    assert_success(&shown);
    assert_eq!(body(&shown), expected);
}

#[tokio::test]
async fn agent_revoke_sends_no_request_body() {
    let proxy = Proxy::start().await;
    proxy.login().await;
    proxy.server.agent("No body").await;
    let output = proxy
        .server
        .run(&["agent", "revoke", "No body", "--yes", "--output", "json"])
        .await;
    assert_success(&output);
    assert_eq!(body(&output), json!({"data": null, "meta": {}}));
    let requests = proxy.requests.lock().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["body"], "");
    assert_eq!(requests[0]["content_type"], "");
    assert_eq!(requests[0]["status"], 200);
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

#[tokio::test]
async fn login_device_waits_at_slower_pace_when_server_requests_slow_down() {
    let proxy = Proxy::configured(json!({"device_poll_interval_seconds": 1,
        "device_request_lifetime_seconds": 25}))
    .await;
    proxy.fault.store(DEVICE_SLOW_DOWN, Ordering::SeqCst);
    let login = proxy.device_login().await;
    proxy.wait_for_polls(2).await;
    proxy.server.decide_device(&login, "approve").await;
    assert_success(&login.finish().await);
    let requests = proxy.requests.lock().await;
    assert_eq!(requests.len(), 3);
    assert!(requests[0]["elapsed_ms"].as_u64().unwrap() >= 1000);
    assert_eq!(requests[0]["status"], 400);
    assert_eq!(requests[1]["status"], 400);
    assert_eq!(requests[2]["status"], 200);
    assert!(requests[1]["elapsed_ms"].as_u64().unwrap() >= 6000);
    assert!(requests[2]["elapsed_ms"].as_u64().unwrap() >= 6000);
    assert_success(&proxy.server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn login_device_retries_when_connection_is_interrupted() {
    let proxy = Proxy::configured(json!({"device_poll_interval_seconds": 2})).await;
    proxy.fault.store(DEVICE_DISCONNECT, Ordering::SeqCst);
    let login = proxy.device_login().await;
    proxy.wait_for_polls(3).await;
    proxy.server.decide_device(&login, "approve").await;
    assert_success(&login.finish().await);
    let requests = proxy.requests.lock().await;
    assert!(requests[0]["status"].is_null());
    assert_poll_order(&requests[1..], &[]);
    assert_minimum_spacing(&requests[..1], 2000);
    assert_minimum_spacing(&requests[1..2], 4000);
    assert_minimum_spacing(&requests[2..], 2000);
    assert_success(&proxy.server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn login_device_retries_when_server_returns_errors() {
    let proxy = Proxy::configured(json!({"device_poll_interval_seconds": 1,
        "device_request_lifetime_seconds": 20}))
    .await;
    proxy.fault.store(DEVICE_UNAVAILABLE, Ordering::SeqCst);
    let login = proxy.device_login().await;
    proxy.wait_for_polls(3).await;
    proxy.server.decide_device(&login, "approve").await;
    assert_success(&login.finish().await);
    let requests = proxy.requests.lock().await;
    assert_poll_order(&requests, &[503, 500]);
    assert_minimum_spacing(&requests[1..2], 2000);
    assert_minimum_spacing(&requests[2..3], 4000);
    assert_minimum_spacing(&requests[3..], 1000);
    assert_success(&proxy.server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn login_device_restores_slower_pace_when_server_recovers() {
    let proxy = Proxy::configured(json!({"device_poll_interval_seconds": 1,
        "device_request_lifetime_seconds": 45}))
    .await;
    proxy.fault.store(DEVICE_SLOW_DOWN_OUTAGE, Ordering::SeqCst);
    let login = proxy.device_login().await;
    proxy.wait_for_polls(3).await;
    proxy.server.decide_device(&login, "approve").await;
    assert_success(&login.finish().await);
    let requests = proxy.requests.lock().await;
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests
            .iter()
            .map(|r| r["status"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        vec![400, 503, 400, 200]
    );
    assert!(requests[1]["elapsed_ms"].as_u64().unwrap() >= 6000);
    assert!(requests[2]["elapsed_ms"].as_u64().unwrap() >= 12000);
    assert!((6000..9000).contains(&requests[3]["elapsed_ms"].as_u64().unwrap()));
    assert_success(&proxy.server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn login_device_retries_at_server_pace_when_rate_limited() {
    let proxy = Proxy::configured(json!({"device_poll_interval_seconds": 1})).await;
    proxy.fault.store(DEVICE_RATE_LIMIT, Ordering::SeqCst);
    let login = proxy.device_login().await;
    proxy.wait_for_polls(3).await;
    proxy.server.decide_device(&login, "approve").await;
    assert_success(&login.finish().await);
    let requests = proxy.requests.lock().await;
    assert_poll_order(&requests, &[429, 429]);
    assert_minimum_spacing(&requests[1..2], 2000);
    assert_minimum_spacing(&requests[2..], 1000);
    assert_success(&proxy.server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn login_device_waits_until_date_when_rate_limited() {
    let proxy = Proxy::configured(json!({"device_poll_interval_seconds": 1})).await;
    proxy.fault.store(DEVICE_RATE_LIMIT_DATE, Ordering::SeqCst);
    let login = proxy.device_login().await;
    proxy.wait_for_polls(3).await;
    proxy.server.decide_device(&login, "approve").await;
    assert_success(&login.finish().await);
    let requests = proxy.requests.lock().await;
    assert_poll_order(&requests, &[429, 429]);
    assert_minimum_spacing(&requests[1..2], 2000);
    assert_minimum_spacing(&requests[2..], 1000);
    assert_success(&proxy.server.run(&["agent", "list"]).await);
}

#[tokio::test]
async fn login_device_stops_at_expiry_when_token_request_does_not_return() {
    let proxy = Proxy::configured(json!({"device_poll_interval_seconds": 1,
        "device_request_lifetime_seconds": 3}))
    .await;
    proxy.fault.store(DEVICE_STALL, Ordering::SeqCst);
    let started = Instant::now();
    let login = proxy.device_login().await;
    let output = login.finish().await;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("expired_token"));
    assert!(started.elapsed() < HTTP_DEADLINE);
    assert!(!proxy.server.home().join(CREDENTIALS_FILE).exists());
}

#[tokio::test]
async fn login_device_refuses_metadata_endpoint_on_another_server() {
    let proxy = Proxy::start().await;
    proxy.fault.store(CHANGE_DEVICE_METADATA, Ordering::SeqCst);
    let output = proxy
        .server
        .run(&["login", &proxy.url, "--device", "--output", "json"])
        .await;
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("Open this address"));
    assert!(!proxy.server.home().join(CREDENTIALS_FILE).exists());
}

const CHANGE_METADATA: u8 = 1;
const LOSE_REFRESH: u8 = 2;
const REDIRECT_CALL: u8 = 3;
const DEVICE_SLOW_DOWN: u8 = 4;
const DEVICE_DISCONNECT: u8 = 5;
const DEVICE_RATE_LIMIT: u8 = 6;
const DEVICE_RATE_LIMIT_AGAIN: u8 = 7;
const DEVICE_STALL: u8 = 8;
const CHANGE_DEVICE_METADATA: u8 = 9;
const DEVICE_UNAVAILABLE: u8 = 10;
const DEVICE_SERVER_ERROR: u8 = 11;
const DEVICE_SLOW_DOWN_OUTAGE: u8 = 12;
const DEVICE_OUTAGE_ONCE: u8 = 13;
const DEVICE_RATE_LIMIT_DATE: u8 = 14;
const DEVICE_RATE_LIMIT_PAST_DATE: u8 = 15;
const HTTP_DEADLINE: Duration = Duration::from_secs(8);

struct Proxy {
    server: Server,
    url: String,
    fault: Arc<AtomicU8>,
    calls: Arc<AtomicUsize>,
    refreshes: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Value>>>,
    task: JoinHandle<()>,
}

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct ProxyState {
    upstream: String,
    fault: Arc<AtomicU8>,
    calls: Arc<AtomicUsize>,
    refreshes: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Value>>>,
    redirect_path: PathBuf,
    last_poll: Arc<Mutex<Option<Instant>>>,
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
        let calls = Arc::new(AtomicUsize::new(0));
        let refreshes = Arc::new(AtomicUsize::new(0));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let state = ProxyState::builder()
            .upstream(server.url.clone())
            .fault(fault.clone())
            .calls(calls.clone())
            .refreshes(refreshes.clone())
            .requests(requests.clone())
            .redirect_path(server.directory.path().join("redirect"))
            .last_poll(Arc::new(Mutex::new(None)))
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
            calls,
            refreshes,
            requests,
            task,
        }
    }

    #[expect(
        clippy::unwrap_used,
        reason = "The scenario requires its CLI process to start."
    )]
    async fn device_login(&self) -> DeviceSigningIn {
        let child = self
            .server
            .cli(&["login", &self.url, "--device", "--output", "json"])
            .spawn()
            .unwrap();
        DeviceSigningIn::start(child).await
    }

    #[expect(
        clippy::unwrap_used,
        reason = "The scenario must observe polling before its deadline."
    )]
    async fn wait_for_polls(&self, count: usize) {
        timeout(WAIT, async {
            while self.requests.lock().await.len() < count {
                sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap();
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

fn assert_poll_order(requests: &[Value], prefix: &[u16]) {
    assert!(requests.len() >= prefix.len() + 2);
    for (request, status) in requests.iter().zip(prefix) {
        assert_eq!(request["status"], *status);
    }
    for request in &requests[prefix.len()..requests.len() - 1] {
        assert_eq!(request["status"], 400);
    }
    assert_eq!(requests[requests.len() - 1]["status"], 200);
}

fn assert_minimum_spacing(requests: &[Value], milliseconds: u64) {
    for request in requests {
        assert!(
            request["elapsed_ms"]
                .as_u64()
                .is_some_and(|elapsed| elapsed >= milliseconds),
            "Poll spacing {} ms is below {milliseconds} ms",
            request["elapsed_ms"]
        );
    }
}

async fn forward(mut socket: TcpStream, state: ProxyState) -> io::Result<()> {
    let (request, headers, body) = read_request(&mut socket).await?;
    state.calls.fetch_add(1, Ordering::SeqCst);
    let refresh =
        headers.starts_with("POST /oauth/token ") && body.starts_with(b"grant_type=refresh_token");
    if refresh {
        state.refreshes.fetch_add(1, Ordering::SeqCst);
    }
    let device = headers.starts_with("POST /oauth/token ") && body.starts_with(b"grant_type=urn");
    let started = Instant::now();
    if headers.starts_with("POST /oauth/device ") {
        *state.last_poll.lock().await = Some(started);
    }
    let response = upstream(&state.upstream, &request, &body).await?;
    let response = if device {
        device_response(&state, &request, &body, response).await?
    } else {
        change_response(&state, &headers, response)?
    };
    record_request(&state.requests, &headers, &body, &response).await;
    if device {
        let mut last = state.last_poll.lock().await;
        let elapsed = last.map(|last| started.duration_since(last).as_millis());
        *last = Some(started);
        let status = String::from_utf8_lossy(&response)
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<u16>().ok());
        state
            .requests
            .lock()
            .await
            .push(json!({"status": status, "elapsed_ms": elapsed}));
    }
    if refresh && take_fault(&state.fault, LOSE_REFRESH) {
        return Ok(());
    }
    socket.write_all(&response).await
}

async fn upstream(address: &str, request: &[u8], body: &[u8]) -> io::Result<Vec<u8>> {
    let mut target = TcpStream::connect(address.trim_start_matches("http://")).await?;
    let mut request = request[..request.len() - 2].to_vec();
    request.extend_from_slice(b"Connection: close\r\n\r\n");
    request.extend_from_slice(body);
    target.write_all(&request).await?;
    let mut response = Vec::new();
    target.read_to_end(&mut response).await?;
    Ok(response)
}

async fn device_response(
    state: &ProxyState,
    request: &[u8],
    body: &[u8],
    response: Vec<u8>,
) -> io::Result<Vec<u8>> {
    if take_fault(&state.fault, DEVICE_SLOW_DOWN)
        || state
            .fault
            .compare_exchange(
                DEVICE_SLOW_DOWN_OUTAGE,
                DEVICE_OUTAGE_ONCE,
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_ok()
    {
        return upstream(&state.upstream, request, body).await;
    }
    if let Some(response) = dated_rate_limit_response(&state.fault) {
        return Ok(response);
    }
    if let Some(response) = server_error_response(&state.fault) {
        return Ok(response);
    }
    if take_fault(&state.fault, DEVICE_DISCONNECT) {
        return Ok(Vec::new());
    }
    if take_fault(&state.fault, DEVICE_STALL) {
        sleep(WAIT).await;
    }
    if state
        .fault
        .compare_exchange(
            DEVICE_RATE_LIMIT,
            DEVICE_RATE_LIMIT_AGAIN,
            Ordering::SeqCst,
            Ordering::SeqCst,
        )
        .is_ok()
    {
        return Ok(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 2\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec());
    }
    if take_fault(&state.fault, DEVICE_RATE_LIMIT_AGAIN) {
        return Ok(b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec());
    }
    Ok(response)
}

fn dated_rate_limit_response(fault: &AtomicU8) -> Option<Vec<u8>> {
    let deadline = if take_fault(fault, DEVICE_RATE_LIMIT_DATE) {
        fault.store(DEVICE_RATE_LIMIT_PAST_DATE, Ordering::SeqCst);
        SystemTime::now() + Duration::from_secs(3)
    } else if take_fault(fault, DEVICE_RATE_LIMIT_PAST_DATE) {
        UNIX_EPOCH
    } else {
        return None;
    };
    let date = fmt_http_date(deadline);
    Some(format!("HTTP/1.1 429 Too Many Requests\r\nRetry-After: {date}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes())
}

fn server_error_response(fault: &AtomicU8) -> Option<Vec<u8>> {
    let (status, content_type, body) = if take_fault(fault, DEVICE_UNAVAILABLE) {
        fault.store(DEVICE_SERVER_ERROR, Ordering::SeqCst);
        (
            "503 Service Unavailable",
            "text/html",
            "<html>Temporarily unavailable</html>",
        )
    } else if take_fault(fault, DEVICE_SERVER_ERROR) {
        (
            "500 Internal Server Error",
            "application/json",
            "{\"error\":\"server_error\"}",
        )
    } else if take_fault(fault, DEVICE_OUTAGE_ONCE) {
        (
            "503 Service Unavailable",
            "text/html",
            "<html>Temporarily unavailable</html>",
        )
    } else {
        return None;
    };
    Some(format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes())
}

fn change_response(state: &ProxyState, headers: &str, response: Vec<u8>) -> io::Result<Vec<u8>> {
    if headers.starts_with("GET /api/v1/agents") && take_fault(&state.fault, REDIRECT_CALL) {
        let location = fs::read_to_string(&state.redirect_path)?;
        return Ok(format!("HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").into_bytes());
    }
    if !headers.starts_with("GET /.well-known/") {
        return Ok(response);
    }
    let field = if take_fault(&state.fault, CHANGE_METADATA) {
        "token_endpoint"
    } else if take_fault(&state.fault, CHANGE_DEVICE_METADATA) {
        "device_authorization_endpoint"
    } else {
        return Ok(response);
    };
    let start = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap_or(response.len());
    let mut metadata: Value =
        serde_json::from_slice(&response[start.saturating_add(4).min(response.len())..])?;
    metadata[field] = json!("http://127.0.0.1:1/oauth/device");
    let body = metadata.to_string();
    Ok(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).into_bytes())
}

fn take_fault(fault: &AtomicU8, value: u8) -> bool {
    fault
        .compare_exchange(value, 0, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

async fn record_request(requests: &Mutex<Vec<Value>>, headers: &str, body: &[u8], response: &[u8]) {
    if headers.starts_with("DELETE /api/v1/agents/") || headers.starts_with("POST /api/v1/spaces ")
    {
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
