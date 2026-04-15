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
    let response = app.oneshot(
        Request::builder()
            .method("POST")
            .uri("/bots")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "TestBot");
    assert!(json["id"].is_string());
}

#[tokio::test]
async fn test_list_bots_returns_empty() {
    let (app, _pool) = common::test_app().await;
    let response = app.oneshot(
        Request::builder()
            .uri("/bots")
            .body(Body::empty())
            .unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json.as_array().unwrap().is_empty());
}
