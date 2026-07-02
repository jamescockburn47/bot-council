// Test code may unwrap/expect/panic — that is what asserts are
// (CLAUDE.md: unwrap() allowed in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn list_admins_empty_on_fresh_db() {
    let (app, _pool) = common::test_app().await;
    let req = common::admin_auth(Request::builder().method("GET").uri("/admins"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn add_admin_requires_clerk_format() {
    let (app, _pool) = common::test_app().await;
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/admins")
            .header("content-type", "application/json"),
    )
    .body(Body::from(r#"{"user_id":"not_a_clerk_id"}"#))
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn add_admin_happy_path() {
    let (app, pool) = common::test_app().await;
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/admins")
            .header("content-type", "application/json"),
    )
    .body(Body::from(r#"{"user_id":"user_2abc123"}"#))
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["user_id"], "user_2abc123");

    // Verify row exists in DB.
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM admins WHERE user_id = ?")
        .bind("user_2abc123")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn add_admin_idempotent() {
    let (app, _pool) = common::test_app().await;
    for _ in 0..3 {
        let req = common::admin_auth(
            Request::builder()
                .method("POST")
                .uri("/admins")
                .header("content-type", "application/json"),
        )
        .body(Body::from(r#"{"user_id":"user_2same"}"#))
        .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }
    // Should still have only one row.
    let req = common::admin_auth(Request::builder().method("GET").uri("/admins"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn remove_admin_happy_path() {
    let (app, pool) = common::test_app().await;
    sqlx::query("INSERT INTO admins (user_id) VALUES ('user_2tobeDeleted')")
        .execute(&pool)
        .await
        .unwrap();

    let req = common::admin_auth(
        Request::builder()
            .method("DELETE")
            .uri("/admins/user_2tobeDeleted"),
    )
    .body(Body::empty())
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM admins")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn remove_admin_not_found_returns_404() {
    let (app, _pool) = common::test_app().await;
    let req = common::admin_auth(
        Request::builder()
            .method("DELETE")
            .uri("/admins/user_2nonexistent"),
    )
    .body(Body::empty())
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_endpoints_reject_unauthenticated() {
    let (app, _pool) = common::test_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/admins")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_users_marks_admin_status() {
    let (app, pool) = common::test_app().await;
    sqlx::query("INSERT INTO seen_users (user_id) VALUES ('user_2admin')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO seen_users (user_id) VALUES ('user_2regular')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO admins (user_id) VALUES ('user_2admin')")
        .execute(&pool)
        .await
        .unwrap();

    let req = common::admin_auth(Request::builder().method("GET").uri("/users"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let arr: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(arr.len(), 2);

    let admin = arr.iter().find(|u| u["user_id"] == "user_2admin").unwrap();
    let regular = arr
        .iter()
        .find(|u| u["user_id"] == "user_2regular")
        .unwrap();
    assert_eq!(admin["is_admin"], json!(true));
    assert_eq!(regular["is_admin"], json!(false));
}
