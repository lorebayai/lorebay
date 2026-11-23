use std::env;
use std::fs;
use std::time::Duration;

use lorebay_lib_types::Secret;
use reqwest::redirect::Policy;
use reqwest::retry;
use reqwest::{Certificate, Client, Method, Response, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use typed_builder::TypedBuilder;
use url::{Host, Url};

use crate::constants::{
    CLI_CONNECTION, CLI_USAGE, CLIENT_ID_FIELD, CODE, CODE_VERIFIER, DEVICE_CODE, ERROR,
    GRANT_TYPE, LOGIN_HINT, MESSAGE, REFRESH_TOKEN, SIGN_IN_PATH, STATUS,
};

use crate::credentials::{CliSignIn, Store, now};
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
    pub device_authorization_endpoint: Option<String>,
}

#[derive(Clone, Debug, Deserialize, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub(crate) struct Tokens {
    pub access_token: Secret<String>,
    pub refresh_token: Secret<String>,
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
                .map_err(|_| Failure::local(CLI_CONNECTION, "Could not read SSL_CERT_FILE."))?;
            let certificates = Certificate::from_pem_bundle(&pem).map_err(|_| {
                Failure::local(
                    CLI_CONNECTION,
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
        ]
        .into_iter()
        .chain(metadata.device_authorization_endpoint.iter())
        {
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
            .filter(|(key, _)| matches!(*key, CODE | CODE_VERIFIER | DEVICE_CODE | REFRESH_TOKEN))
            .map(|(_, value)| *value)
            .collect();
        tokens_from_value(decode(response, &secrets).await?)
    }

    pub async fn authorized(
        &self,
        store: &Store,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value> {
        let mut sign_in = store.load()?.ok_or_else(Failure::login_required)?;
        let mut refreshed = false;
        if sign_in.expires_at <= now() {
            let _lock = store.lock().await?;
            sign_in = store.load()?.ok_or_else(Failure::login_required)?;
            if sign_in.expires_at <= now() {
                sign_in = self.refresh(store, sign_in).await?;
                refreshed = true;
            }
        }
        let mut response = self.call(&sign_in, method.clone(), path, body).await?;
        if response.status() == StatusCode::UNAUTHORIZED && !refreshed {
            let _lock = store.lock().await?;
            let latest = store.load()?.ok_or_else(Failure::login_required)?;
            sign_in = if latest.access_token.expose() != sign_in.access_token.expose()
                && latest.expires_at > now()
            {
                latest
            } else {
                self.refresh(store, latest).await?
            };
            drop(_lock);
            response = self.call(&sign_in, method, path, body).await?;
        }
        decode(
            response,
            &[
                sign_in.access_token.expose().as_str(),
                sign_in.refresh_token.expose().as_str(),
            ],
        )
        .await
    }

    pub async fn refresh(&self, store: &Store, sign_in: CliSignIn) -> Result<CliSignIn> {
        let server = server_url(&sign_in.server_url)?;
        same_server(&server, &sign_in.token_endpoint)?;
        let result = self
            .tokens(
                &sign_in.token_endpoint,
                &[
                    (GRANT_TYPE, REFRESH_TOKEN),
                    (CLIENT_ID_FIELD, CLIENT_ID),
                    (REFRESH_TOKEN, sign_in.refresh_token.expose().as_str()),
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
        let sign_in = sign_in_from_tokens(sign_in.server_url, sign_in.token_endpoint, tokens);
        store.save(&sign_in)?;
        Ok(sign_in)
    }

    pub async fn call(
        &self,
        sign_in: &CliSignIn,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Response> {
        let server = server_url(&sign_in.server_url)?;
        let url = endpoint(&server, path)?;
        let mut request = self
            .client
            .request(method, url)
            .bearer_auth(sign_in.access_token.expose());
        if let Some(body) = body {
            request = request.json(body);
        }
        request.send().await.map_err(Failure::connection)
    }
}

pub(crate) async fn logout(store: &Store) -> Result<()> {
    let _lock = store.lock().await?;
    let loaded = store.load();
    if loaded.is_err() {
        store.remove()?;
    }
    let Some(sign_in) = loaded? else {
        return Ok(());
    };
    let result = end_sign_in(store, sign_in).await;
    store.remove()?;
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.exit_code() == 4 || error.is_ended() => Ok(()),
        Err(error) => {
            Err(error
                .context("Local credentials removed; the sign-in was not ended on the server."))
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
            CLI_CONNECTION,
            "The server returned an endpoint on another server.",
        ));
    }
    Ok(())
}

pub(crate) fn sign_in_from_tokens(
    server_url: String,
    token_endpoint: String,
    tokens: Tokens,
) -> CliSignIn {
    CliSignIn::builder()
        .server_url(server_url)
        .token_endpoint(token_endpoint)
        .access_token(tokens.access_token)
        .refresh_token(tokens.refresh_token)
        .expires_at(now().saturating_add(tokens.expires_in))
        .build()
}

pub(crate) fn tokens_from_value(value: Value) -> Result<Tokens> {
    let tokens: Tokens = serde_json::from_value(value).map_err(|_| invalid_response())?;
    if tokens.access_token.expose().is_empty()
        || tokens.refresh_token.expose().is_empty()
        || !tokens.token_type.eq_ignore_ascii_case("bearer")
        || tokens.expires_in == 0
    {
        return Err(invalid_response());
    }
    Ok(tokens)
}

pub(crate) async fn decode(response: Response, secrets: &[&str]) -> Result<Value> {
    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .map_err(|_| invalid_response())?;
    decode_body(status, body, secrets)
}

pub(crate) fn decode_body(status: StatusCode, body: Value, secrets: &[&str]) -> Result<Value> {
    if status.is_success() {
        return Ok(body);
    }
    let mut body = if let Some(code) = body[ERROR].as_str() {
        json!({(CODE): code, (MESSAGE): body["error_description"].as_str().unwrap_or("OAuth request failed.")})
    } else {
        body
    };
    if !body.is_object() || !body[CODE].is_string() {
        return Err(invalid_response());
    }
    redact(&mut body, secrets);
    if body[STATUS] == StatusCode::UNAUTHORIZED.as_u16() {
        body[MESSAGE] = json!(LOGIN_HINT);
    }
    Err(Failure::server(body))
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

async fn end_sign_in(store: &Store, mut sign_in: CliSignIn) -> Result<()> {
    let http = Http::new()?;
    let mut refreshed = false;
    if sign_in.expires_at <= now() {
        sign_in = http.refresh(store, sign_in).await?;
        refreshed = true;
    }
    let mut response = http
        .call(&sign_in, Method::DELETE, SIGN_IN_PATH, None)
        .await?;
    if response.status() == StatusCode::UNAUTHORIZED && !refreshed {
        sign_in = http.refresh(store, sign_in).await?;
        response = http
            .call(&sign_in, Method::DELETE, SIGN_IN_PATH, None)
            .await?;
    }
    decode(
        response,
        &[
            sign_in.access_token.expose().as_str(),
            sign_in.refresh_token.expose().as_str(),
        ],
    )
    .await?;
    Ok(())
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
    Failure::local(CLI_CONNECTION, "The server returned an invalid response.")
}

fn invalid_url() -> Failure {
    Failure::local(
        CLI_USAGE,
        "Use an HTTPS server origin, or HTTP on localhost, 127.0.0.0/8 or ::1; omit credentials, paths, queries and fragments.",
    )
}
