mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn auth(token: &str) -> String {
    format!("Bearer {token}")
}

#[tokio::test]
async fn test_me_returns_profile() {
    let (app, _pool) = common::test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/me")
                .header("Authorization", auth("user_alice"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["user_id"], "user_alice");
    assert_eq!(json["role"], "member");
}

#[tokio::test]
async fn test_admin_registry_promote_and_demote() {
    let (app, _pool) = common::test_app().await;

    // Seed seen users. The auth backdoor (test_mode) upserts seen_users on
    // every request, so a GET /me call is enough.
    for who in ["user_alice", "user_bob"] {
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/me")
                    .header("Authorization", auth(who))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    // Promote user_alice using the admin backdoor.
    let promote = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admins")
                .header("Authorization", auth("admin:admin-token"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({ "user_id": "user_alice" })).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(promote.status(), StatusCode::CREATED);

    // Verify admin registry reflects the promotion.
    let list_admins = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/admins")
                .header("Authorization", auth("admin:admin-token"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_admins.status(), StatusCode::OK);
    let body = axum::body::to_bytes(list_admins.into_body(), usize::MAX)
        .await
        .unwrap();
    let admins: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(admins.as_array().unwrap().len(), 1);
    assert_eq!(admins[0]["user_id"], "user_alice");

    // /users should reflect the admin state for both seeded users.
    let list_users = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/users")
                .header("Authorization", auth("admin:admin-token"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_users.status(), StatusCode::OK);
    let body = axum::body::to_bytes(list_users.into_body(), usize::MAX)
        .await
        .unwrap();
    let users: Value = serde_json::from_slice(&body).unwrap();
    assert!(users.as_array().unwrap().len() >= 2);
    let alice = users
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["user_id"] == "user_alice")
        .unwrap();
    assert_eq!(alice["is_admin"], true);
    let bob = users
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["user_id"] == "user_bob")
        .unwrap();
    assert_eq!(bob["is_admin"], false);

    // Demote alice.
    let demote = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/admins/user_alice")
                .header("Authorization", auth("admin:admin-token"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(demote.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn test_bot_submission_and_approval_flow() {
    // Spin up a mock bot server so the smoke-test that runs during approval
    // has something to hit and can return a well-formed DebateRoundResponse.
    let bot_server = MockServer::start().await;
    // 5-round smoke gauntlet — approval now runs round 0 through 4 with
    // round-specific schema validation (challenge in round 2,
    // position_change in round 4, integer confidence in rounds 1-4).
    for (round_label, body) in [
        ("round 0", json!({"response": "ack r0"})),
        ("round 1", json!({"response": "ack r1", "confidence": 70})),
        (
            "round 2",
            json!({
                "response": "ack r2",
                "confidence": 65,
                "challenge": {
                    "claim_targeted": "stub",
                    "counter_evidence": "stub",
                    "type": "factual"
                }
            }),
        ),
        ("round 3", json!({"response": "ack r3", "confidence": 60})),
        (
            "round 4",
            json!({
                "response": "ack r4",
                "confidence": 75,
                "position_change": {
                    "changed": false,
                    "from_summary": "stub",
                    "to_summary": "stub",
                    "reason": "stub"
                }
            }),
        ),
    ] {
        Mock::given(method("POST"))
            .and(path("/debate"))
            .and(wiremock::matchers::body_string_contains(round_label))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&bot_server)
            .await;
    }
    let bot_url = format!("{}/debate", bot_server.uri());

    let (app, _pool) = common::test_app().await;

    // Participant submits a bot.
    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bots")
                .header("Authorization", auth("user_alice"))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "name": "Arguer",
                        "endpoint_url": bot_url,
                        "token": "bot-secret",
                        "model_family": "claude",
                        "description": "My submission"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(create.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(created["status"], "pending");
    assert_eq!(created["submitted_by"], "user_alice");
    let bot_id = created["id"].as_str().unwrap().to_string();

    // User can see their own submissions.
    let mine = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/bots/my-submissions")
                .header("Authorization", auth("user_alice"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(mine.status(), StatusCode::OK);
    let body = axum::body::to_bytes(mine.into_body(), usize::MAX)
        .await
        .unwrap();
    let mine_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(mine_json.as_array().unwrap().len(), 1);
    assert_eq!(mine_json[0]["status"], "pending");

    // Admin approves; smoke test hits the mock bot server above.
    let approve = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/bots/{bot_id}/approve"))
                .header("Authorization", auth("admin:admin-token"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(approve.status(), StatusCode::OK);
    let body = axum::body::to_bytes(approve.into_body(), usize::MAX)
        .await
        .unwrap();
    let approved: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(approved["status"], "active");
    assert_eq!(approved["reviewed_by"], "admin-token");
}
