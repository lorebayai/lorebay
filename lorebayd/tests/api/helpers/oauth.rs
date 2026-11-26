use std::fmt::Write;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::{Method, Response, StatusCode};
use sea_orm::sea_query::{Alias, Expr, Query};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, ExprTrait};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use typed_builder::TypedBuilder;
use url::Url;

use super::{Server, authorize_for_client};

pub const REDIRECT_URI: &str = "http://127.0.0.1:46137/callback";
pub const PKCE_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct AuthorizedAgent {
    pub client_id: String,
    pub tokens: Value,
}

impl Server {
    pub async fn assert_token_issuer(&self, tokens: &Value) {
        let metadata = self.get("/.well-known/oauth-authorization-server").await;
        assert_eq!(metadata.status(), StatusCode::OK);
        let metadata = metadata.json::<Value>().await.unwrap();
        let payload = tokens["access_token"]
            .as_str()
            .unwrap()
            .split('.')
            .nth(1)
            .unwrap();
        let claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
        assert_eq!(claims["iss"], metadata["issuer"]);
    }

    pub async fn register_client(&self, name: &str) -> String {
        let response = self
            .json(
                Method::POST,
                "/oauth/register",
                json!({"client_name": name, "redirect_uris": [REDIRECT_URI],
                    "token_endpoint_auth_method": "none",
                    "grant_types": ["authorization_code", "refresh_token"]}),
                None,
            )
            .await;
        assert_eq!(response.status(), StatusCode::CREATED);
        response.json::<Value>().await.unwrap()["client_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub async fn approve_code(&self, client_id: &str, name: Option<&str>) -> String {
        let tokens = self.tokens().await;
        self.approve_code_for(client_id, name, tokens["access_token"].as_str().unwrap())
            .await
    }

    pub async fn approve_code_for(
        &self,
        client_id: &str,
        name: Option<&str>,
        person_token: &str,
    ) -> String {
        let response = self
            .json(
                Method::POST,
                "/api/v1/authorization-decision",
                json!({"client_id": client_id, "redirect_uri": REDIRECT_URI,
                    "response_type": "code", "code_challenge": PKCE_CHALLENGE,
                    "code_challenge_method": "S256", "decision": "approve",
                    "agent_name": name}),
                Some(person_token),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.json::<Value>().await.unwrap();
        let redirect = Url::parse(body["data"]["redirect_url"].as_str().unwrap()).unwrap();
        redirect
            .query_pairs()
            .find(|(key, _)| key == "code")
            .unwrap()
            .1
            .into_owned()
    }

    pub async fn exchange_code(&self, client_id: &str, code: &str) -> Response {
        self.token_request(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id),
            ("code", code),
            ("redirect_uri", REDIRECT_URI),
            ("code_verifier", PKCE_VERIFIER),
            (
                "resource",
                &format!("{}/mcp", self.public_url.trim_end_matches('/')),
            ),
        ])
        .await
    }

    pub async fn token_request(&self, parameters: &[(&str, &str)]) -> Response {
        self.client
            .post(format!("http://{}/oauth/token", self.address))
            .form(parameters)
            .send()
            .await
            .unwrap()
    }

    pub async fn refresh_oauth(&self, client_id: &str, token: &str) -> Response {
        self.token_request(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", token),
            (
                "resource",
                &format!("{}/mcp", self.public_url.trim_end_matches('/')),
            ),
        ])
        .await
    }

    pub async fn authorize_agent(&self, name: &str) -> AuthorizedAgent {
        let client_id = self.register_client(name).await;
        let tokens = self.tokens().await;
        authorize_for_client(self, &client_id, name, &tokens).await
    }

    pub async fn agent_names(&self, tokens: &Value) -> Vec<String> {
        let response = self
            .json(
                Method::GET,
                "/api/v1/agents",
                Value::Null,
                tokens["access_token"].as_str(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        response.json::<Value>().await.unwrap()["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|agent| agent["name"].as_str().unwrap().to_owned())
            .collect()
    }

    pub async fn expire_oauth_credential(&self, table: &str, column: &str) {
        let database = self.open_database().await;
        let statement = Query::update()
            .table(Alias::new(table))
            .value(Alias::new(column), Expr::value(0_i64))
            .to_owned();
        database.execute(&statement).await.unwrap();
        database.close().await.unwrap();
    }

    pub async fn remove_agent_sign_ins(&self) {
        let database = self.open_database().await;
        let statement = Query::delete()
            .from_table(Alias::new("agent_sign_in"))
            .to_owned();
        database.execute(&statement).await.unwrap();
        database.close().await.unwrap();
    }

    pub async fn set_agent_sign_in_name(&self, current_name: &str, name: &str) {
        let database = self.open_database().await;
        let statement = Query::update()
            .table(Alias::new("agent_sign_in"))
            .value(Alias::new("name"), Expr::value(name))
            .and_where(Expr::col(Alias::new("name")).eq(current_name))
            .to_owned();
        let result = database.execute(&statement).await.unwrap();
        assert_eq!(result.rows_affected(), 1);
        database.close().await.unwrap();
    }

    pub async fn set_oauth_code_agent_name(&self, code: &str, name: &str) {
        let hash = Sha256::digest(code.as_bytes()).iter().fold(
            String::with_capacity(64),
            |mut out, byte| {
                let _ = write!(out, "{byte:02x}");
                out
            },
        );
        let database = self.open_database().await;
        let statement = Query::update()
            .table(Alias::new("oauth_code"))
            .value(Alias::new("agent_name"), Expr::value(name))
            .and_where(Expr::col(Alias::new("code_hash")).eq(hash))
            .to_owned();
        let result = database.execute(&statement).await.unwrap();
        assert_eq!(result.rows_affected(), 1);
        database.close().await.unwrap();
    }

    async fn open_database(&self) -> DatabaseConnection {
        let path = self.directory.path().join("data/lorebay.sqlite3");
        Database::connect(format!("sqlite://{}", path.display()))
            .await
            .unwrap()
    }
}

pub async fn assert_taken(response: Response) {
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(!response.headers().contains_key("location"));
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"type": "about:blank", "title": "Bad Request", "status": 400,
            "code": "request.invalid_body", "errors": [{"field": "agent_name",
                "code": "taken", "message": "You already have an agent with this name."}]})
    );
}

const PKCE_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
