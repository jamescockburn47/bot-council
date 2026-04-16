mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use serde_json::{json, Value};

#[tokio::test]
async fn test_create_bot_returns_201() {
    let (app, _pool) = common::test_app().await;
    let body = json!({
        "name": "TestBot",
        "endpoint_url": "http://localhost:9999/debate",
        "token": "secret123"
    });
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/bots")
            .header("content-type", "application/json"),
    )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "TestBot");
    assert!(json["id"].is_string());
}

#[tokio::test]
async fn test_list_bots_returns_empty() {
    let (app, _pool) = common::test_app().await;
    let req = common::admin_auth(
        Request::builder()
            .uri("/bots"),
    )
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn reject_with_short_reason_returns_400() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, token_hash, token_ciphertext, status) \
         VALUES ('b1', 'B1', 'https://example.com/d', '', X'00', 'pending')"
    ).execute(&pool).await.unwrap();

    let req = common::admin_auth(
        Request::builder().method("PATCH").uri("/bots/b1/reject")
            .header("content-type", "application/json"),
    ).body(Body::from(r#"{"reason":"short"}"#)).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn reject_with_valid_reason_sets_status_and_reason() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, token_hash, token_ciphertext, status) \
         VALUES ('b2', 'B2', 'https://example.com/d', '', X'00', 'pending')"
    ).execute(&pool).await.unwrap();

    let req = common::admin_auth(
        Request::builder().method("PATCH").uri("/bots/b2/reject")
            .header("content-type", "application/json"),
    ).body(Body::from(r#"{"reason":"endpoint returned garbage on all test rounds"}"#)).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "rejected");
    assert!(json["rejection_reason"].as_str().unwrap().contains("endpoint returned garbage"));
}

#[tokio::test]
async fn deactivate_pending_bot_returns_409() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, token_hash, token_ciphertext, status) \
         VALUES ('b3', 'B3', 'https://example.com/d', '', X'00', 'pending')"
    ).execute(&pool).await.unwrap();

    let req = common::admin_auth(
        Request::builder().method("PATCH").uri("/bots/b3/deactivate"),
    ).body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn submitted_token_is_encrypted_in_db() {
    let (app, pool) = common::test_app().await;
    let body = json!({
        "name": "TokenTest",
        "endpoint_url": "https://example.com/debate",
        "token": "s3cr3t-bearer-xyz"
    });
    let req = common::admin_auth(
        Request::builder().method("POST").uri("/bots")
            .header("content-type", "application/json"),
    ).body(Body::from(serde_json::to_string(&body).unwrap())).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let (ciphertext, hash): (Option<Vec<u8>>, Option<String>) = sqlx::query_as(
        "SELECT token_ciphertext, token_hash FROM bots WHERE name = 'TokenTest'"
    ).fetch_one(&pool).await.unwrap();
    assert!(ciphertext.is_some(), "ciphertext should be populated");
    // Legacy token_hash column is still NOT NULL in schema; new rows write
    // empty string. The column is dropped entirely in a follow-up migration.
    assert_eq!(hash.as_deref(), Some(""), "new rows populate legacy hash with empty string");
    let ct = ciphertext.unwrap();
    assert!(ct.len() > 12, "ciphertext must be longer than 12-byte nonce");
    assert!(!ct.windows(5).any(|w| w == b"s3cr3"),
            "plaintext token leaked into ciphertext");
}

#[tokio::test]
async fn submit_with_http_url_returns_400() {
    let (app, _pool) = common::test_app().await;
    let body = json!({
        "name": "NoTLS",
        "endpoint_url": "http://evil.example.com/debate",
        "token": "any"
    });
    let req = common::admin_auth(
        Request::builder().method("POST").uri("/bots")
            .header("content-type", "application/json"),
    ).body(Body::from(serde_json::to_string(&body).unwrap())).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
