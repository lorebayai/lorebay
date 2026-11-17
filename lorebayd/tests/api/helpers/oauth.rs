use reqwest::{Method, Response, StatusCode};
use sea_orm::sea_query::{Alias, Expr, Query};
use sea_orm::{ConnectionTrait, Database};
use serde_json::{Value, json};
use typed_builder::TypedBuilder;
use url::Url;

use super::Server;

pub const REDIRECT_URI: &str = "http://127.0.0.1:46137/callback";
pub const PKCE_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

#[derive(Clone, Debug, TypedBuilder)]
#[builder(field_defaults(setter(into)))]
pub struct AuthorizedAgent {
    pub client_id: String,
    pub tokens: Value,
}

impl Server {
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
        let response = self
            .json(
                Method::POST,
                "/api/v1/authorization-decision",
                json!({"client_id": client_id, "redirect_uri": REDIRECT_URI,
                    "response_type": "code", "code_challenge": PKCE_CHALLENGE,
                    "code_challenge_method": "S256", "decision": "approve",
                    "agent_name": name}),
                tokens["access_token"].as_str(),
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
            ("resource", &format!("http://{}/mcp", self.address)),
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
            ("resource", &format!("http://{}/mcp", self.address)),
        ])
        .await
    }

    pub async fn authorize_agent(&self, name: &str) -> AuthorizedAgent {
        let client_id = self.register_client(name).await;
        let code = self.approve_code(&client_id, Some(name)).await;
        let response = self.exchange_code(&client_id, &code).await;
        assert_eq!(response.status(), StatusCode::OK);
        AuthorizedAgent::builder()
            .client_id(client_id)
            .tokens(response.json::<Value>().await.unwrap())
            .build()
    }

    pub async fn expire_oauth_credential(&self, table: &str, column: &str) {
        let path = self.directory.path().join("data/lorebay.sqlite3");
        let database = Database::connect(format!("sqlite://{}", path.display()))
            .await
            .unwrap();
        let statement = Query::update()
            .table(Alias::new(table))
            .value(Alias::new(column), Expr::value(0_i64))
            .to_owned();
        database.execute(&statement).await.unwrap();
        database.close().await.unwrap();
    }
}

const PKCE_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
