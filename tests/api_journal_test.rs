//! Integration tests for the admin journal endpoint (/admin/events).

// Test code may unwrap/expect/panic — that is what asserts are
// (CLAUDE.md: unwrap() allowed in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn journal_requires_admin() {
    let (app, _pool) = common::test_app().await;
    let req = Request::builder()
        .method("GET")
        .uri("/admin/events")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_ne!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn journal_lists_events_newest_first_with_filters() {
    let (app, pool) = common::test_app().await;
    bot_council::observability::events::record_event(
        &pool,
        "service_started",
        bot_council::observability::events::EventScope::default(),
        "",
        None,
    )
    .await;
    bot_council::observability::events::record_event(
        &pool,
        "quorum_not_met",
        bot_council::observability::events::EventScope {
            label: "Debate \"test topic\"",
            debate_id: Some("d-1"),
            bot_id: None,
        },
        "Only 1 debater answered the opening round.",
        None,
    )
    .await;

    let req = common::admin_auth(Request::builder().method("GET").uri("/admin/events"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(rows.len(), 2);
    // Newest first: the quorum event was recorded second.
    assert_eq!(rows[0]["event_kind"], "quorum_not_met");
    assert_eq!(rows[0]["severity"], "problem");
    assert!(rows[0]["narrative"].as_str().unwrap().contains("cancelled"));
    assert!(rows[0]["suggested_action"].as_str().is_some());

    // Severity filter.
    let req = common::admin_auth(
        Request::builder()
            .method("GET")
            .uri("/admin/events?severity=problem"),
    )
    .body(Body::empty())
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["event_kind"], "quorum_not_met");

    // Limit clamp: limit=0 still returns at least one row (clamped to 1).
    let req = common::admin_auth(
        Request::builder()
            .method("GET")
            .uri("/admin/events?limit=0"),
    )
    .body(Body::empty())
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&body).unwrap();
    assert_eq!(rows.len(), 1);
}
