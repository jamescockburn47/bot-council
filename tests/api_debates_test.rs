mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use serde_json::{json, Value};

async fn seed_bots(app: &mut axum::Router) -> Vec<String> {
    let mut ids = Vec::new();
    for i in 0..3 {
        let body = json!({"name": format!("Bot{}", i), "endpoint_url": format!("http://localhost:999{}/debate", i), "token": format!("token{}", i)});
        let req = common::admin_auth(
            Request::builder().method("POST").uri("/bots").header("content-type", "application/json"),
        )
            .body(Body::from(serde_json::to_string(&body).unwrap())).unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        ids.push(json["id"].as_str().unwrap().to_string());
    }
    ids
}

#[tokio::test]
async fn test_create_debate_returns_201() {
    let (mut app, _pool) = common::test_app().await;
    let bot_ids = seed_bots(&mut app).await;
    let body = json!({"topic": "Should AI-generated evidence be admissible in court?", "bot_ids": bot_ids});
    let req = common::admin_auth(
        Request::builder().method("POST").uri("/debates").header("content-type", "application/json"),
    )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["topic"], "Should AI-generated evidence be admissible in court?");
    assert_eq!(json["bots"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_create_debate_rejects_insufficient_bots() {
    let (app, _pool) = common::test_app().await;
    let body = json!({"topic": "Test topic", "bot_ids": []});
    let req = common::admin_auth(
        Request::builder().method("POST").uri("/debates").header("content-type", "application/json"),
    )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_debate_not_found() {
    let (app, _pool) = common::test_app().await;
    let req = common::admin_auth(
        Request::builder().uri("/debates/nonexistent"),
    )
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_transcript_not_found() {
    let (app, _pool) = common::test_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/debates/nonexistent/transcript")
                .header("Authorization", "Bearer test-admin-token")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_get_synthesis_not_found() {
    let (app, _pool) = common::test_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/debates/nonexistent/synthesis")
                .header("Authorization", "Bearer test-admin-token")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn create_debate_without_auth_returns_401() {
    let (app, _pool) = common::test_app().await;
    let body = serde_json::json!({ "topic": "X" });
    let req = Request::builder()
        .method("POST")
        .uri("/debates")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
