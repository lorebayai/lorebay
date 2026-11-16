use reqwest::StatusCode;
use serde_json::{Value, json};

use crate::helpers::Server;

#[tokio::test]
async fn call_web_api_fails_with_not_found_when_path_is_unknown() {
    let server = Server::start().await;
    for path in ["/api/v1/unknown", "/api/v1", "/unknown"] {
        let response = server.get(path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        assert_eq!(
            response.json::<Value>().await.unwrap(),
            json!({
                "type": "about:blank", "title": "Not Found", "status": 404,
                "code": "http.not_found", "errors": []
            })
        );
    }
    server.stop().await;
}
