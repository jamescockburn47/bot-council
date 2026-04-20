mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use serde_json::Value;

#[tokio::test]
async fn test_health_returns_ok() {
    let (app, _pool) = common::test_app().await;
    let response = app.oneshot(
        Request::builder().uri("/health").body(Body::empty()).unwrap(),
    ).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_diag_health_alias_returns_ok() {
    let (app, _pool) = common::test_app().await;
    let response = app
        .oneshot(Request::builder().uri("/diag/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_diag_models_requires_admin() {
    let (app, _pool) = common::test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/diag/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_diag_models_returns_routing_snapshot() {
    let (app, _pool) = common::test_app().await;
    let response = app
        .oneshot(
            common::admin_auth(Request::builder().uri("/diag/models"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json.get("analysis_base_url").is_some());
    assert!(json.get("final_synthesis_base_url").is_some());
    assert_eq!(json["analysis_max_concurrency"], 2);
}

#[tokio::test]
async fn test_config_json_returns_frontend_runtime_config() {
    let (app, _pool) = common::test_app().await;
    let response = app
        .oneshot(Request::builder().uri("/config.json").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    // Public by design — no auth required. Shape is the frontend bootstrap contract.
    assert_eq!(json["publishable_key"], "pk_test_Y29uZmlnLmpzb24tdGVzdA");
    assert_eq!(json["api_base"], "/api");
    assert_eq!(json["sentry_environment"], "test");
    assert!(json.get("release").is_some());
}
