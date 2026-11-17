use std::collections::HashMap;
use std::str;
use std::thread;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::signal::ctrl_c;
use tokio::time::timeout;
use typed_builder::TypedBuilder;
use url::Url;

use crate::credentials::Store;
use crate::error::{Failure, Result};
use crate::http::{CLIENT_ID, Http, Metadata, server_url, session_from_tokens};
use crate::output;

pub(crate) async fn login(store: &Store, address: &str, no_browser: bool) -> Result<()> {
    let server = server_url(address)?;
    let http = Http::new()?;
    let metadata = http.metadata(&server).await?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let attempt = Attempt::builder()
        .state(random()?)
        .verifier(random()?)
        .redirect(format!("http://{}/callback", listener.local_addr()?))
        .metadata(metadata)
        .build();
    let url = authorization_url(&attempt)?;
    output::hint(&format!("Open this URL to sign in: {url}"))?;
    if !no_browser {
        open_browser(url);
    }
    let code = tokio::select! {
        result = timeout(LOGIN_WAIT, wait_for_callback(listener, &attempt)) => result
            .map_err(|_| Failure::local("cli.timeout", "Sign-in timed out."))??,
        _ = ctrl_c() => return Err(Failure::local("cli.cancelled", "Sign-in cancelled.")),
    };
    let tokens = http
        .tokens(
            &attempt.metadata.token_endpoint,
            &[
                ("grant_type", "authorization_code"),
                ("client_id", CLIENT_ID),
                ("code", &code),
                ("redirect_uri", &attempt.redirect),
                ("code_verifier", &attempt.verifier),
            ],
        )
        .await?;
    let session = session_from_tokens(server.to_string(), attempt.metadata.token_endpoint, tokens);
    let _lock = store.lock().await?;
    store.save(&session)
}

const LOGIN_WAIT: Duration = Duration::from_secs(300);
const CALLBACK_WAIT: Duration = Duration::from_secs(2);
const MAX_CALLBACK_BYTES: usize = 8192;
const RANDOM_BYTES: usize = 32;

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
struct Attempt {
    state: String,
    verifier: String,
    redirect: String,
    metadata: Metadata,
}

fn random() -> Result<String> {
    let mut bytes = [0; RANDOM_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| {
        Failure::local(
            "cli.credentials",
            "Could not generate secure sign-in randomness.",
        )
    })?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn authorization_url(attempt: &Attempt) -> Result<Url> {
    let mut url = Url::parse(&attempt.metadata.authorization_endpoint)
        .map_err(|_| Failure::local("cli.connection", "Invalid authorization endpoint."))?;
    url.query_pairs_mut()
        .append_pair("client_id", CLIENT_ID)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &attempt.redirect)
        .append_pair("state", &attempt.state)
        .append_pair(
            "code_challenge",
            &URL_SAFE_NO_PAD.encode(Sha256::digest(&attempt.verifier)),
        )
        .append_pair("code_challenge_method", "S256");
    Ok(url)
}

fn open_browser(url: Url) {
    // A launcher waiting on the desktop must not hold up the callback or its deadline.
    let _ = thread::spawn(move || {
        if webbrowser::open(url.as_str()).is_err() {
            let _ = output::hint(
                "Could not open a browser. Open the printed link to continue signing in.",
            );
        }
    });
}

async fn wait_for_callback(listener: TcpListener, attempt: &Attempt) -> Result<String> {
    loop {
        let (stream, _) = listener.accept().await?;
        if let Ok(Ok(Some(result))) = timeout(CALLBACK_WAIT, serve_callback(stream, attempt)).await
        {
            return result;
        }
    }
}

async fn serve_callback(
    mut stream: TcpStream,
    attempt: &Attempt,
) -> Result<Option<Result<String>>> {
    let mut request = Vec::new();
    let mut byte = [0];
    while request.len() < MAX_CALLBACK_BYTES && !request.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).await? == 0 {
            return Ok(None);
        }
        request.push(byte[0]);
    }
    let result = classify_callback(&request, attempt);
    let status = if result.is_some() {
        "200 OK"
    } else {
        "400 Bad Request"
    };
    let body = if result.is_some() {
        "Return to your terminal to finish signing in.\n"
    } else {
        "This request does not match the sign-in.\n"
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    Ok(result)
}

fn classify_callback(request: &[u8], attempt: &Attempt) -> Option<Result<String>> {
    if !request.ends_with(b"\r\n\r\n") {
        return None;
    }
    let line = str::from_utf8(request).ok()?.lines().next()?;
    let mut words = line.split_whitespace();
    if words.next()? != "GET" {
        return None;
    }
    let target = words.next()?;
    if !matches!(words.next()?, "HTTP/1.0" | "HTTP/1.1") || words.next().is_some() {
        return None;
    }
    if !target.starts_with("/callback?") {
        return None;
    }
    let url = Url::parse(&attempt.redirect).ok()?.join(target).ok()?;
    if url.path() != "/callback" {
        return None;
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    let query: HashMap<_, _> = pairs.iter().cloned().collect();
    if pairs.len() != query.len()
        || query.get("state")?.as_ref() != attempt.state
        || query.get("iss")?.as_ref() != attempt.metadata.issuer
    {
        return None;
    }
    match (query.get("code"), query.get("error")) {
        (Some(code), None) if !code.is_empty() => Some(Ok(code.to_string())),
        (None, Some(error)) if !error.is_empty() => Some(Err(Failure::server(json!({
            "code": error, "message": if error == "access_denied" { "Sign-in was declined." }
                else { "OAuth sign-in failed." }
        })))),
        _ => None,
    }
}
