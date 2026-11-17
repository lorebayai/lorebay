use std::time::Duration;

use lorebay_base::Secret;
use reqwest::header::RETRY_AFTER;
use reqwest::{Response, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::signal::ctrl_c;
use tokio::time::{Instant, sleep, timeout_at};
use typed_builder::TypedBuilder;

use crate::constants::{
    CLI_CANCELLED, CLI_CONNECTION, CLIENT_ID_FIELD, CODE, DEVICE_CODE, GRANT_TYPE, MESSAGE,
};
use crate::credentials::Store;
use crate::error::{Failure, Result};
use crate::http::{
    CLIENT_ID, Http, Metadata, Tokens, decode, decode_body, same_server, server_url,
    session_from_tokens, tokens_from_value,
};
use crate::output;

pub(crate) async fn login(store: &Store, address: &str) -> Result<()> {
    let server = server_url(address)?;
    let http = Http::new()?;
    let metadata = http.metadata(&server).await?;
    let started = Instant::now();
    let request = start(&http, &metadata).await?;
    same_server(&server, &request.verification_uri)?;
    same_server(&server, request.verification_uri_complete.expose())?;
    let deadline = started
        .checked_add(Duration::from_secs(request.expires_in))
        .ok_or_else(invalid_request)?;
    output::hint(&format!(
        "Open this address to sign in: {}",
        request.verification_uri
    ))?;
    output::hint(&format!("Enter this code: {}", request.user_code.expose()))?;
    output::hint(&format!(
        "Or open this link: {}",
        request.verification_uri_complete.expose()
    ))?;
    output::hint("Check that the code in the browser matches this code before approving.")?;
    let tokens = tokio::select! {
        result = timeout_at(deadline, poll(&http, &metadata, &request)) =>
            result.map_err(|_| expired())??,
        _ = ctrl_c() => return Err(Failure::local(CLI_CANCELLED, "Sign-in cancelled.")),
    };
    let session = session_from_tokens(server.to_string(), metadata.token_endpoint, tokens);
    let _lock = store.lock().await?;
    store.save(&session)
}

const ACCESS_DENIED: &str = "access_denied";
const AUTHORIZATION_PENDING: &str = "authorization_pending";
const EXPIRED_TOKEN: &str = "expired_token";
const DEFAULT_INTERVAL_SECONDS: u64 = 5;
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const SLOW_DOWN: &str = "slow_down";
const SLOW_DOWN_INCREMENT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct DeviceRequest {
    device_code: Secret<String>,
    user_code: Secret<String>,
    verification_uri: String,
    verification_uri_complete: Secret<String>,
    expires_in: u64,
    #[serde(default = "default_interval")]
    interval: u64,
}

async fn start(http: &Http, metadata: &Metadata) -> Result<DeviceRequest> {
    let endpoint = metadata
        .device_authorization_endpoint
        .as_ref()
        .ok_or_else(|| {
            Failure::local(CLI_CONNECTION, "The server does not offer device sign-in.")
        })?;
    let response = http
        .client
        .post(endpoint)
        .form(&[(CLIENT_ID_FIELD, CLIENT_ID)])
        .send()
        .await
        .map_err(Failure::connection)?;
    let request: DeviceRequest =
        serde_json::from_value(decode(response, &[]).await?).map_err(|_| invalid_request())?;
    if request.device_code.expose().is_empty()
        || request.user_code.expose().is_empty()
        || request.expires_in == 0
        || request.interval == 0
    {
        return Err(invalid_request());
    }
    Ok(request)
}

async fn poll(http: &Http, metadata: &Metadata, request: &DeviceRequest) -> Result<Tokens> {
    let mut interval = Duration::from_secs(request.interval);
    let mut delay = interval;
    loop {
        sleep(delay.min(Duration::from_secs(request.expires_in))).await;
        let response = http
            .client
            .post(&metadata.token_endpoint)
            .form(&[
                (GRANT_TYPE, DEVICE_GRANT),
                (CLIENT_ID_FIELD, CLIENT_ID),
                (DEVICE_CODE, request.device_code.expose().as_str()),
            ])
            .send()
            .await;
        let Ok(response) = response else {
            delay = delay.saturating_mul(2);
            continue;
        };
        let status = response.status();
        if status.is_server_error() {
            delay = delay.saturating_mul(2);
            continue;
        }
        let retry_after = retry_after(&response);
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(_) => {
                delay = delay.saturating_mul(2);
                continue;
            }
        };
        if status == StatusCode::TOO_MANY_REQUESTS {
            delay = interval.max(retry_after);
            continue;
        }
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| invalid_request())?;
        match decode_body(
            status,
            body,
            &[request.device_code.expose(), request.user_code.expose()],
        ) {
            Ok(body) => return tokens_from_value(body),
            Err(error) => match error.body()[CODE].as_str() {
                Some(AUTHORIZATION_PENDING) => {}
                Some(SLOW_DOWN) => interval = interval.saturating_add(SLOW_DOWN_INCREMENT),
                Some(ACCESS_DENIED) => {
                    return Err(Failure::server(json!({
                        (CODE): ACCESS_DENIED, (MESSAGE): "Sign-in was declined."
                    })));
                }
                Some(EXPIRED_TOKEN) => return Err(expired()),
                _ => return Err(error),
            },
        }
        delay = interval;
    }
}

fn retry_after(response: &Response) -> Duration {
    response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_default()
}

fn default_interval() -> u64 {
    DEFAULT_INTERVAL_SECONDS
}

fn expired() -> Failure {
    Failure::server(
        json!({(CODE): EXPIRED_TOKEN, (MESSAGE): "Device sign-in expired; run `lorebay login --device` again."}),
    )
}

fn invalid_request() -> Failure {
    Failure::local(
        CLI_CONNECTION,
        "The server returned an invalid device sign-in response.",
    )
}
