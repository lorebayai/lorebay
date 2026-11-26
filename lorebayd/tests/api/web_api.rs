use reqwest::{Method, StatusCode};
use serde_json::{Value, json};

use crate::helpers::Server;

#[tokio::test]
async fn call_web_api_fails_with_not_found_when_path_is_unknown() {
    let server = Server::start().await;
    let person = server.tokens().await;
    let token = person["access_token"].as_str();
    let expected = json!({
        "type": "about:blank", "title": "Not Found", "status": 404,
        "code": "http.not_found", "errors": []
    });
    for path in [
        "/api/v1/unknown",
        "/api/v1/agents/unknown/extra",
        "/api/v1",
        "/unknown",
    ] {
        for response in [
            server.get(path).await,
            server.json(Method::DELETE, path, Value::Null, token).await,
        ] {
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(
                response.headers()["content-type"],
                "application/problem+json"
            );
            assert_eq!(response.json::<Value>().await.unwrap(), expected);
        }
    }
    server.stop().await;
}
