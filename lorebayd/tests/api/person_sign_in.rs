use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::time::sleep;

use crate::helpers::{Server, Settings, TEST_PASSWORD, TEST_USERNAME};

#[tokio::test]
async fn log_in_returns_access_and_refresh_tokens_when_password_matches() {
    let server = Server::start().await;
    let response = server.log_in(TEST_USERNAME, TEST_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["meta"], json!({}));
    let tokens = &body["data"];
    server.assert_token_issuer(tokens).await;
    let access = tokens["access_token"].as_str().unwrap();
    assert_eq!(access.split('.').count(), 3);
    assert_eq!(tokens["refresh_token"].as_str().unwrap().len(), 44);
    assert_eq!(tokens["token_type"], "Bearer");
    assert_eq!(tokens["expires_in"], 900);
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    let response = server.log_out(Some(access)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({"data": null, "meta": {}})
    );
    server.stop().await;
}

#[tokio::test]
async fn log_in_fails_when_password_is_wrong() {
    let server = Server::start().await;
    let wrong = server.log_in(TEST_USERNAME, "wrong password").await;
    let unknown = server.log_in("unknown", TEST_PASSWORD).await;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.headers()["content-type"], "application/problem+json");
    let body: Value = wrong.json().await.unwrap();
    assert_eq!(body, unknown.json::<Value>().await.unwrap());
    assert_eq!(
        body,
        json!({"type": "about:blank", "title": "Unauthorized", "status": 401, "code": "auth.unauthorized", "errors": []})
    );
    assert_eq!(
        server.log_in(TEST_USERNAME, TEST_PASSWORD).await.status(),
        StatusCode::OK
    );
    server.stop().await;
}

#[tokio::test]
async fn refresh_login_session_returns_new_tokens() {
    let server = Server::configured(|config| {
        config.public_url = "https://lorebay.test:8443/".to_owned();
    })
    .await;
    let mut tokens = server.tokens().await;
    for _ in 0..2 {
        let response = server
            .refresh(tokens["refresh_token"].as_str().unwrap())
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["meta"], json!({}));
        let new = &body["data"];
        server.assert_token_issuer(new).await;
        assert_ne!(new["access_token"], tokens["access_token"]);
        assert_ne!(new["refresh_token"], tokens["refresh_token"]);
        tokens = new.clone();
    }
    let response = server
        .refresh(tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    let new = &body["data"];
    assert_eq!(
        server.log_out(new["access_token"].as_str()).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        server
            .refresh(new["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    server.stop().await;
}

#[tokio::test]
async fn refresh_login_session_ends_login_session_when_refresh_token_was_already_exchanged() {
    let server = Server::start().await;
    let tokens = server.tokens().await;
    let independent = server.tokens().await;
    let response = server
        .refresh(tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let refreshed = response.json::<Value>().await.unwrap()["data"].clone();
    let unknown = server.refresh("unknown-refresh-token").await;
    let replay = server
        .refresh(tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(replay.status(), unknown.status());
    assert_eq!(
        replay.headers()["content-type"],
        unknown.headers()["content-type"]
    );
    assert_eq!(
        replay.json::<Value>().await.unwrap(),
        unknown.json::<Value>().await.unwrap()
    );
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    assert_eq!(
        server
            .log_out(refreshed["access_token"].as_str())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .refresh(refreshed["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .log_out(independent["access_token"].as_str())
            .await
            .status(),
        StatusCode::OK
    );
    let signed_in = server.tokens().await;
    assert_eq!(
        server
            .log_out(signed_in["access_token"].as_str())
            .await
            .status(),
        StatusCode::OK
    );
    server.stop().await;
}

#[tokio::test]
async fn refresh_login_session_fails_when_login_session_is_expired() {
    let server = Server::start().await;
    let tokens = server.tokens().await;
    server.expire_login_sessions().await;
    assert_eq!(
        server
            .refresh(tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .log_out(tokens["access_token"].as_str())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    server.stop().await;
}

#[tokio::test]
async fn log_out_ends_login_session() {
    let server = Server::start().await;
    let tokens = server.tokens().await;
    assert_eq!(tokens["expires_in"], 900);
    let access = tokens["access_token"].as_str();
    assert_eq!(server.log_out(access).await.status(), StatusCode::OK);
    assert_eq!(
        server.log_out(access).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .refresh(tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    server.stop().await;
}

#[tokio::test]
async fn call_web_api_fails_when_access_token_signature_is_invalid() {
    let server = Server::start().await;
    let tokens = server.tokens().await;
    let access = tokens["access_token"].as_str().unwrap();
    let (header_and_claims, signature) = access.rsplit_once('.').unwrap();
    let first = if signature.starts_with('A') { "B" } else { "A" };
    let forged = format!("{header_and_claims}.{first}{}", &signature[1..]);
    for token in [None, Some("not-a-jwt"), Some(forged.as_str())] {
        let response = server.log_out(token).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()["www-authenticate"], "Bearer");
    }
    assert_eq!(server.log_out(Some(access)).await.status(), StatusCode::OK);
    server.stop().await;
}

#[tokio::test]
async fn log_in_fails_when_account_was_removed_from_users_file() {
    let server = Server::start().await;
    let tokens = server.tokens().await;
    let directory = server.stop().await;
    let server = Server::restart_with_users(directory, &[]).await;
    assert_eq!(
        server.log_in(TEST_USERNAME, TEST_PASSWORD).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .log_out(tokens["access_token"].as_str())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .refresh(tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    server.stop().await;
}

#[tokio::test]
async fn log_in_succeeds_when_removed_account_is_added_back() {
    let server = Server::start().await;
    let tokens = server.tokens().await;
    let directory = server.stop().await;
    let server = Server::restart_with_users(directory, &[]).await;
    let directory = server.stop().await;
    let server = Server::restart_with_users(directory, &[(TEST_USERNAME, "new password")]).await;
    assert_eq!(
        server.log_in(TEST_USERNAME, "new password").await.status(),
        StatusCode::OK
    );
    assert_eq!(
        server.log_in(TEST_USERNAME, TEST_PASSWORD).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .log_out(tokens["access_token"].as_str())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .refresh(tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    server.stop().await;
}

#[tokio::test]
async fn log_in_fails_when_rate_limit_is_exceeded() {
    let server = Server::start_with_settings(Settings::builder().rate_limit(1_u32).build()).await;
    let tokens = server.tokens().await;
    let response = server.log_in(TEST_USERNAME, TEST_PASSWORD).await;
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        (1..=61).contains(
            &response.headers()["retry-after"]
                .to_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
        )
    );
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    assert_eq!(
        response.json::<Value>().await.unwrap()["code"],
        "auth.rate_limited"
    );
    assert_eq!(
        server
            .refresh(tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::OK
    );
    server.stop().await;
}

#[tokio::test]
async fn call_web_api_fails_when_access_token_has_expired() {
    let server =
        Server::start_with_settings(Settings::builder().access_seconds(1_u64).build()).await;
    let tokens = server.tokens().await;
    // Refreshed tokens need the default lifetime to stay valid for the final call on slow runners.
    let directory = server.stop().await;
    let server = Server::restart(directory).await;
    sleep(Duration::from_secs(2)).await;
    assert_eq!(
        server
            .log_out(tokens["access_token"].as_str())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let response = server
        .refresh(tokens["refresh_token"].as_str().unwrap())
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let new: Value = response.json().await.unwrap();
    assert_eq!(
        server
            .log_out(new["data"]["access_token"].as_str())
            .await
            .status(),
        StatusCode::OK
    );
    server.stop().await;
}

#[tokio::test]
async fn log_in_masks_unknown_username_when_hash_uses_nondefault_parameters() {
    let server = Server::start_with_nondefault_hash().await;
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    for _ in 0..3 {
        let started = Instant::now();
        let response = server.log_in(TEST_USERNAME, "incorrect password").await;
        known.push(started.elapsed());
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body: Value = response.json().await.unwrap();
        let started = Instant::now();
        let response = server.log_in("unknown", "incorrect password").await;
        unknown.push(started.elapsed());
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.json::<Value>().await.unwrap(), body);
    }
    let known = known.into_iter().min().unwrap();
    let unknown = unknown.into_iter().min().unwrap();
    assert!(
        known <= unknown * 3 && unknown <= known * 3,
        "known={known:?}, unknown={unknown:?}"
    );
    assert_eq!(
        server.log_in(TEST_USERNAME, TEST_PASSWORD).await.status(),
        StatusCode::OK
    );
    server.stop().await;
}

#[tokio::test]
async fn refresh_login_session_ends_login_session_when_older_replaced_refresh_token_is_presented() {
    let server = Server::start().await;
    let original = server.tokens().await;
    let independent = server.tokens().await;
    let mut tokens = original.clone();
    for _ in 0..2 {
        let response = server
            .refresh(tokens["refresh_token"].as_str().unwrap())
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        tokens = response.json::<Value>().await.unwrap()["data"].clone();
    }
    let replay = server
        .refresh(original["refresh_token"].as_str().unwrap())
        .await;
    let unknown = server.refresh("unknown-token").await;
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        replay.json::<Value>().await.unwrap(),
        unknown.json::<Value>().await.unwrap()
    );
    assert_eq!(
        server
            .log_out(tokens["access_token"].as_str())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .refresh(tokens["refresh_token"].as_str().unwrap())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        server
            .log_out(independent["access_token"].as_str())
            .await
            .status(),
        StatusCode::OK
    );
    let fresh = server.tokens().await;
    assert_eq!(
        server
            .log_out(fresh["access_token"].as_str())
            .await
            .status(),
        StatusCode::OK
    );
    server.stop().await;
}
