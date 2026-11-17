use std::env;
use std::fs;
use std::time::Duration;

use reqwest::redirect::Policy;
use reqwest::retry;
use reqwest::{Certificate, Client, Method, Response, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use typed_builder::TypedBuilder;
use url::{Host, Url};

use crate::credentials::{Secret, Session, Store, now};
use crate::error::{Failure, Result};

pub(crate) const CLIENT_ID: &str = "lorebay-cli";

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Http {
    pub client: Client,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Metadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Tokens {
    pub access_token: Secret,
    pub refresh_token: Secret,
    pub expires_in: u64,
    pub token_type: String,
}

impl Http {
    pub fn new() -> Result<Self> {
        let mut builder = Client::builder()
            .redirect(Policy::none())
            .retry(retry::never())
            .timeout(REQUEST_TIMEOUT);
        if let Some(path) = env::var_os("SSL_CERT_FILE") {
            let pem = fs::read(path)
                .map_err(|_| Failure::local("cli.connection", "Could not read SSL_CERT_FILE."))?;
            let certificates = Certificate::from_pem_bundle(&pem).map_err(|_| {
                Failure::local(
                    "cli.connection",
                    "SSL_CERT_FILE must contain PEM certificate authorities.",
                )
            })?;
            builder = builder.tls_certs_only(certificates);
        }
        Ok(Self::builder()
            .client(builder.build().map_err(Failure::connection)?)
            .build())
    }

    pub async fn metadata(&self, server: &Url) -> Result<Metadata> {
        let url = endpoint(server, "/.well-known/oauth-authorization-server")?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(Failure::connection)?;
        let metadata: Metadata =
            serde_json::from_value(decode(response, &[]).await?).map_err(|_| invalid_response())?;
        for address in [
            &metadata.issuer,
            &metadata.authorization_endpoint,
            &metadata.token_endpoint,
        ] {
            same_server(server, address)?;
        }
        Ok(metadata)
    }

    pub async fn tokens(&self, url: &str, form: &[(&str, &str)]) -> Result<Tokens> {
        let response = self
            .client
            .post(url)
            .form(form)
            .send()
            .await
            .map_err(Failure::connection)?;
        let secrets: Vec<_> = form
            .iter()
            .filter(|(key, _)| matches!(*key, "code" | "code_verifier" | "refresh_token"))
            .map(|(_, value)| *value)
            .collect();
        let tokens: Tokens = serde_json::from_value(decode(response, &secrets).await?)
            .map_err(|_| invalid_response())?;
        if tokens.access_token.value.is_empty()
            || tokens.refresh_token.value.is_empty()
            || !tokens.token_type.eq_ignore_ascii_case("bearer")
            || tokens.expires_in == 0
        {
            return Err(invalid_response());
        }
        Ok(tokens)
    }

    pub async fn authorized(&self, store: &Store, method: Method, path: &str) -> Result<Value> {
        let mut session = store.load()?.ok_or_else(Failure::login_required)?;
        let mut refreshed = false;
        if session.expires_at <= now() {
            let _lock = store.lock().await?;
            session = store.load()?.ok_or_else(Failure::login_required)?;
            if session.expires_at <= now() {
                session = self.refresh(store, session).await?;
                refreshed = true;
            }
        }
        let mut response = self.call(&session, method.clone(), path).await?;
        if response.status() == StatusCode::UNAUTHORIZED && !refreshed {
            let _lock = store.lock().await?;
            let latest = store.load()?.ok_or_else(Failure::login_required)?;
            session = if latest.access_token.value != session.access_token.value
                && latest.expires_at > now()
            {
                latest
            } else {
                self.refresh(store, latest).await?
            };
            drop(_lock);
            response = self.call(&session, method, path).await?;
        }
        decode(
            response,
            &[&session.access_token.value, &session.refresh_token.value],
        )
        .await
    }

    pub async fn refresh(&self, store: &Store, session: Session) -> Result<Session> {
        let server = server_url(&session.server_url)?;
        same_server(&server, &session.token_endpoint)?;
        let result = self
            .tokens(
                &session.token_endpoint,
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", CLIENT_ID),
                    ("refresh_token", &session.refresh_token.value),
                ],
            )
            .await;
        let tokens = match result {
            Ok(tokens) => tokens,
            Err(error) if error.is_ended() => {
                store.remove()?;
                return Err(error.require_login());
            }
            Err(error) => return Err(error),
        };
        let session = session_from_tokens(session.server_url, session.token_endpoint, tokens);
        store.save(&session)?;
        Ok(session)
    }

    pub async fn call(&self, session: &Session, method: Method, path: &str) -> Result<Response> {
        let server = server_url(&session.server_url)?;
        let url = endpoint(&server, path)?;
        self.client
            .request(method, url)
            .bearer_auth(&session.access_token.value)
            .send()
            .await
            .map_err(Failure::connection)
    }
}

pub(crate) async fn logout(store: &Store) -> Result<()> {
    let _lock = store.lock().await?;
    let loaded = store.load();
    if loaded.is_err() {
        store.remove()?;
    }
    let Some(session) = loaded? else {
        return Ok(());
    };
    let result = end_session(store, session).await;
    store.remove()?;
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.exit_code() == 4 || error.is_ended() => Ok(()),
        Err(error) => {
            Err(error
                .context("Local credentials removed; the session was not ended on the server."))
        }
    }
}

pub(crate) fn server_url(text: &str) -> Result<Url> {
    let url = Url::parse(text).map_err(|_| invalid_url())?;
    let loopback = match url.host() {
        Some(Host::Domain("localhost")) => true,
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        _ => false,
    };
    if url.host().is_none()
        || !(url.scheme() == "https" || url.scheme() == "http" && loopback)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(invalid_url());
    }
    Ok(url)
}

pub(crate) fn same_server(server: &Url, address: &str) -> Result<()> {
    let url = Url::parse(address).map_err(|_| invalid_response())?;
    if url.origin() != server.origin()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Failure::local(
            "cli.connection",
            "The server returned an endpoint on another server.",
        ));
    }
    Ok(())
}

pub(crate) fn session_from_tokens(
    server_url: String,
    token_endpoint: String,
    tokens: Tokens,
) -> Session {
    Session::builder()
        .server_url(server_url)
        .token_endpoint(token_endpoint)
        .access_token(tokens.access_token)
        .refresh_token(tokens.refresh_token)
        .expires_at(now().saturating_add(tokens.expires_in))
        .build()
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

async fn end_session(store: &Store, mut session: Session) -> Result<()> {
    let http = Http::new()?;
    let mut refreshed = false;
    if session.expires_at <= now() {
        session = http.refresh(store, session).await?;
        refreshed = true;
    }
    let mut response = http
        .call(&session, Method::DELETE, "/api/v1/login-session")
        .await?;
    if response.status() == StatusCode::UNAUTHORIZED && !refreshed {
        session = http.refresh(store, session).await?;
        response = http
            .call(&session, Method::DELETE, "/api/v1/login-session")
            .await?;
    }
    decode(
        response,
        &[&session.access_token.value, &session.refresh_token.value],
    )
    .await?;
    Ok(())
}

async fn decode(response: Response, secrets: &[&str]) -> Result<Value> {
    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .map_err(|_| invalid_response())?;
    if status.is_success() {
        return Ok(body);
    }
    let mut body = if let Some(code) = body["error"].as_str() {
        json!({"code": code, "message": body["error_description"].as_str().unwrap_or("OAuth request failed.")})
    } else {
        body
    };
    if !body.is_object() || !body["code"].is_string() {
        return Err(invalid_response());
    }
    redact(&mut body, secrets);
    if body["status"] == 401 {
        body["message"] = json!("Run `lorebay login` again.");
    }
    Err(Failure::server(body))
}

fn redact(value: &mut Value, secrets: &[&str]) {
    match value {
        Value::String(text) => {
            for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
                *text = text.replace(secret, "<redacted>");
            }
        }
        Value::Array(values) => values.iter_mut().for_each(|value| redact(value, secrets)),
        Value::Object(values) => values.values_mut().for_each(|value| redact(value, secrets)),
        _ => {}
    }
}

fn endpoint(server: &Url, path: &str) -> Result<Url> {
    let url = server.join(path).map_err(|_| invalid_url())?;
    same_server(server, url.as_str())?;
    Ok(url)
}

fn invalid_response() -> Failure {
    Failure::local("cli.connection", "The server returned an invalid response.")
}

fn invalid_url() -> Failure {
    Failure::local(
        "cli.usage",
        "Use an HTTPS server origin, or HTTP on localhost, 127.0.0.0/8 or ::1; omit credentials, paths, queries and fragments.",
    )
}
