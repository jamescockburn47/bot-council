// Test code may unwrap/expect/panic — that is what asserts are
// (CLAUDE.md: unwrap() allowed in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

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
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "TestBot");
    assert!(json["id"].is_string());
}

#[tokio::test]
async fn test_list_bots_returns_empty() {
    let (app, _pool) = common::test_app().await;
    let req = common::admin_auth(Request::builder().uri("/bots"))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn reject_with_short_reason_returns_400() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, token_ciphertext, status) \
         VALUES ('b1', 'B1', 'https://example.com/d', X'00', 'pending')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let req = common::admin_auth(
        Request::builder()
            .method("PATCH")
            .uri("/bots/b1/reject")
            .header("content-type", "application/json"),
    )
    .body(Body::from(r#"{"reason":"short"}"#))
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn reject_with_valid_reason_sets_status_and_reason() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, token_ciphertext, status) \
         VALUES ('b2', 'B2', 'https://example.com/d', X'00', 'pending')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let req = common::admin_auth(
        Request::builder()
            .method("PATCH")
            .uri("/bots/b2/reject")
            .header("content-type", "application/json"),
    )
    .body(Body::from(
        r#"{"reason":"endpoint returned garbage on all test rounds"}"#,
    ))
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "rejected");
    assert!(
        json["rejection_reason"]
            .as_str()
            .unwrap()
            .contains("endpoint returned garbage")
    );
}

#[tokio::test]
async fn deactivate_pending_bot_returns_409() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, token_ciphertext, status) \
         VALUES ('b3', 'B3', 'https://example.com/d', X'00', 'pending')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let req = common::admin_auth(
        Request::builder()
            .method("PATCH")
            .uri("/bots/b3/deactivate"),
    )
    .body(Body::empty())
    .unwrap();
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
        Request::builder()
            .method("POST")
            .uri("/bots")
            .header("content-type", "application/json"),
    )
    .body(Body::from(serde_json::to_string(&body).unwrap()))
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let (ciphertext,): (Option<Vec<u8>>,) =
        sqlx::query_as("SELECT token_ciphertext FROM bots WHERE name = 'TokenTest'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(ciphertext.is_some(), "ciphertext should be populated");
    let ct = ciphertext.unwrap();
    assert!(
        ct.len() > 12,
        "ciphertext must be longer than 12-byte nonce"
    );
    assert!(
        !ct.windows(5).any(|w| w == b"s3cr3"),
        "plaintext token leaked into ciphertext"
    );
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
        Request::builder()
            .method("POST")
            .uri("/bots")
            .header("content-type", "application/json"),
    )
    .body(Body::from(serde_json::to_string(&body).unwrap()))
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_bot_endpoint_returns_ok_true_for_healthy_bot() {
    let (app, pool) = common::test_app().await;
    let bot_server = MockServer::start().await;
    // Full-5-round smoke gauntlet. Each round requires round-specific
    // fields: confidence (R1-R4), challenge (R2), position_change (R4).
    // Mock one matcher per round so the test bot behaves like a bot
    // that correctly implements all five round schemas.
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("Smoke test round 0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"response": "round0 ok"})))
        .mount(&bot_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("Smoke test round 1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": "round1 ok", "confidence": 70
        })))
        .mount(&bot_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("Smoke test round 2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": "round2 ok",
            "confidence": 65,
            "challenge": {
                "claim_targeted": "The 60% figure is biased",
                "counter_evidence": "Unbiased data shows 15%",
                "type": "factual"
            }
        })))
        .mount(&bot_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("Smoke test round 3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": "round3 ok", "confidence": 60
        })))
        .mount(&bot_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("Smoke test round 4"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": "round4 ok",
            "confidence": 75,
            "position_change": {
                "changed": false,
                "from_summary": "preflight good",
                "to_summary": "preflight good",
                "reason": "opposing arguments did not disprove the reliability gain"
            }
        })))
        .mount(&bot_server)
        .await;
    let endpoint = format!("{}/debate", bot_server.uri());

    sqlx::query("INSERT INTO bots (id, name, endpoint_url, status) VALUES (?, ?, ?, ?)")
        .bind("bot-health-ok")
        .bind("HealthBot")
        .bind(endpoint)
        .bind("active")
        .execute(&pool)
        .await
        .unwrap();

    let req = common::admin_auth(
        Request::builder()
            .method("PATCH")
            .uri("/bots/bot-health-ok/test"),
    )
    .body(Body::empty())
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], true);
}

#[tokio::test]
async fn test_bot_endpoint_returns_ok_false_for_unreachable_bot() {
    let (app, pool) = common::test_app().await;

    sqlx::query("INSERT INTO bots (id, name, endpoint_url, status) VALUES (?, ?, ?, ?)")
        .bind("bot-health-bad")
        .bind("UnreachableBot")
        .bind("http://127.0.0.1:9/debate")
        .bind("active")
        .execute(&pool)
        .await
        .unwrap();

    let req = common::admin_auth(
        Request::builder()
            .method("PATCH")
            .uri("/bots/bot-health-bad/test"),
    )
    .body(Body::empty())
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], false);
    let msg = json["message"].as_str().unwrap();
    assert!(
        msg.contains("Smoke test failed"),
        "unexpected message: {msg}"
    );
}

#[tokio::test]
async fn test_bot_endpoint_fails_when_round1_payload_not_supported() {
    let (app, pool) = common::test_app().await;
    let bot_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("Smoke test round 0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": "round0 ok",
        })))
        .mount(&bot_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("Smoke test round 1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "unexpected": "shape"
        })))
        .mount(&bot_server)
        .await;

    let endpoint = format!("{}/debate", bot_server.uri());
    sqlx::query("INSERT INTO bots (id, name, endpoint_url, status) VALUES (?, ?, ?, ?)")
        .bind("bot-health-round1-bad")
        .bind("Round1BrokenBot")
        .bind(endpoint)
        .bind("active")
        .execute(&pool)
        .await
        .unwrap();

    let req = common::admin_auth(
        Request::builder()
            .method("PATCH")
            .uri("/bots/bot-health-round1-bad/test"),
    )
    .body(Body::empty())
    .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ok"], false);
    let msg = json["message"].as_str().unwrap();
    assert!(msg.contains("response"), "unexpected message: {msg}");
}

#[tokio::test]
async fn list_bots_includes_performance_score_and_suggestions() {
    let (app, pool) = common::test_app().await;
    sqlx::query("INSERT INTO bots (id, name, endpoint_url, status) VALUES (?, ?, ?, ?)")
        .bind("bot-perf-1")
        .bind("PerfBot")
        .bind("https://example.com/debate")
        .bind("active")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO debates (id, topic, status) VALUES (?, ?, ?)")
        .bind("debate-perf-1")
        .bind("topic")
        .bind("complete")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO responses (id, debate_id, round_number, bot_id, response_json, abstained, valid) \
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("resp-perf-1")
    .bind("debate-perf-1")
    .bind(0)
    .bind("bot-perf-1")
    .bind("normal response")
    .bind(false)
    .bind(true)
    .execute(&pool)
    .await
    .unwrap();
    // Should be excluded from performance aggregates.
    sqlx::query("INSERT INTO debates (id, topic, status) VALUES (?, ?, ?)")
        .bind("debate-perf-excluded")
        .bind("Quickfire readiness check for PerfBot")
        .bind("complete")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO responses (id, debate_id, round_number, bot_id, response_json, abstained, valid) \
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("resp-perf-excluded")
    .bind("debate-perf-excluded")
    .bind(0)
    .bind("bot-perf-1")
    .bind("(abstained)")
    .bind(true)
    .bind(false)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO responses (id, debate_id, round_number, bot_id, response_json, abstained, valid) \
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("resp-perf-2")
    .bind("debate-perf-1")
    .bind(1)
    .bind("bot-perf-1")
    .bind("(abstained)")
    .bind(true)
    .bind(true)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO responses (id, debate_id, round_number, bot_id, response_json, abstained, valid) \
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("resp-perf-3")
    .bind("debate-perf-1")
    .bind(2)
    .bind("bot-perf-1")
    .bind("I was unable to formulate a response for this round.")
    .bind(false)
    .bind(true)
    .execute(&pool)
    .await
    .unwrap();

    let req = common::admin_auth(Request::builder().uri("/bots"))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let bot = json
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == "bot-perf-1")
        .expect("bot should be listed");
    assert_eq!(bot["performance"]["total_rounds"], 3);
    assert_eq!(bot["performance"]["debates_participated"], 1);
    assert_eq!(bot["performance"]["abstained_rounds"], 1);
    assert_eq!(bot["performance"]["degraded_rounds"], 1);
    assert!(bot["performance"]["score_out_of_10"].as_f64().unwrap() <= 8.5);
    assert!(
        bot["performance"]["critical_thinking_score_out_of_10"]
            .as_f64()
            .is_some(),
        "expected critical thinking score"
    );
    assert!(
        bot["performance"]["resource_use_score_out_of_10"]
            .as_f64()
            .is_some(),
        "expected resource-use score"
    );
    assert!(
        bot["performance"]["functionality_score_out_of_10"]
            .as_f64()
            .is_some(),
        "expected functionality score"
    );
    assert!(
        bot["performance"]["usefulness_score_out_of_10"]
            .as_f64()
            .is_some(),
        "expected usefulness score"
    );
    assert!(
        bot["performance"]["instruction_following_score_out_of_10"]
            .as_f64()
            .is_some(),
        "expected instruction-following score"
    );
    assert!(
        bot["performance"]["debate_engagement_score_out_of_10"]
            .as_f64()
            .is_some(),
        "expected debate engagement score"
    );
    assert!(
        bot["performance"]["usefulness_score_out_of_10"]
            .as_f64()
            .unwrap()
            < 6.5,
        "expected abstentions/degraded rounds to reduce usefulness"
    );
    assert!(
        bot["performance"]["suggestions"].as_array().unwrap().len() > 0,
        "expected at least one improvement suggestion"
    );
}

#[tokio::test]
async fn bot_analytics_allows_owner_and_admin_but_forbids_other_participants() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, status, submitted_by) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("bot-analytics-auth")
    .bind("OwnerBot")
    .bind("https://example.com/debate")
    .bind("active")
    .bind("owner-user")
    .execute(&pool)
    .await
    .unwrap();

    let owner_req = Request::builder()
        .method("GET")
        .uri("/bots/bot-analytics-auth/analytics")
        .header("authorization", "Bearer owner-user")
        .body(Body::empty())
        .unwrap();
    let owner_res = app.clone().oneshot(owner_req).await.unwrap();
    assert_eq!(owner_res.status(), StatusCode::OK);

    let other_req = Request::builder()
        .method("GET")
        .uri("/bots/bot-analytics-auth/analytics")
        .header("authorization", "Bearer other-user")
        .body(Body::empty())
        .unwrap();
    let other_res = app.clone().oneshot(other_req).await.unwrap();
    assert_eq!(other_res.status(), StatusCode::FORBIDDEN);

    let admin_req = common::admin_auth(
        Request::builder()
            .method("GET")
            .uri("/bots/bot-analytics-auth/analytics"),
    )
    .body(Body::empty())
    .unwrap();
    let admin_res = app.oneshot(admin_req).await.unwrap();
    assert_eq!(admin_res.status(), StatusCode::OK);
}

#[tokio::test]
async fn bot_analytics_returns_recent_debate_rows() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, status, submitted_by) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("bot-analytics-data")
    .bind("DataBot")
    .bind("https://example.com/debate")
    .bind("active")
    .bind("owner-user")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO debates (id, topic, status) VALUES (?, ?, ?)")
        .bind("debate-analytics-data")
        .bind("analytics topic")
        .bind("complete")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO debate_bots (debate_id, bot_id, pseudonym, role) VALUES (?, ?, ?, ?)")
        .bind("debate-analytics-data")
        .bind("bot-analytics-data")
        .bind("Agent A")
        .bind("skeptic")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO responses (id, debate_id, round_number, bot_id, response_json, abstained, valid) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("resp-analytics-data")
    .bind("debate-analytics-data")
    .bind(0)
    .bind("bot-analytics-data")
    .bind("normal response")
    .bind(false)
    .bind(true)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/bots/bot-analytics-data/analytics")
        .header("authorization", "Bearer owner-user")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["bot"]["id"], "bot-analytics-data");
    assert_eq!(
        json["recent_debates"][0]["debate_id"],
        "debate-analytics-data"
    );
    assert_eq!(json["recent_debates"][0]["role"], "skeptic");
}

#[tokio::test]
async fn bot_response_exposes_kind_and_introduction() {
    // Regression guard: the bot response surface (BotResponse DTO) must expose
    // `bot_kind` and `introduction` so clients can tell text-only bots apart
    // from external bots and render the captured introduction. Fields were
    // added by the text-only bot mode work — this test pins them down so a
    // future DTO refactor doesn't silently drop them.
    let (app, pool) = common::test_app().await;

    // Create a text_only bot via the public POST endpoint.
    let body = json!({
        "name": "TextOnlyBot",
        "endpoint_url": "https://example.com/hook",
        "token": "tok",
        "bot_kind": "text_only",
        "description": "test"
    });
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/bots")
            .header("content-type", "application/json"),
    )
    .body(Body::from(body.to_string()))
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let created: Value = serde_json::from_slice(&bytes).unwrap();
    let bot_id = created["id"].as_str().unwrap().to_string();
    // Create-response already carries bot_kind for text_only bots.
    assert_eq!(created["bot_kind"], "text_only");

    // Manually set the introduction on the DB (in production this happens
    // during the approval smoke test; the direct helper keeps this test
    // fast and mock-server-free).
    bot_council::db::queries::set_bot_introduction(
        &pool,
        &bot_id,
        "I am a text-only bot with strong opinions about logging.",
    )
    .await
    .expect("set_bot_introduction");

    // GET bot via the analytics endpoint — the nearest existing "get bot by
    // id" route; wraps BotResponse under `bot`.
    let req = common::admin_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/bots/{bot_id}/analytics")),
    )
    .body(Body::empty())
    .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let fetched: Value = serde_json::from_slice(&bytes).unwrap();

    assert_eq!(fetched["bot"]["bot_kind"], "text_only");
    assert_eq!(
        fetched["bot"]["introduction"],
        "I am a text-only bot with strong opinions about logging."
    );
}

#[tokio::test]
async fn legacy_bot_schema_endpoint_returns_compat_payload() {
    let (app, _pool) = common::test_app().await;
    let req = common::admin_auth(Request::builder().method("GET").uri("/bots/schema"))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["deprecated"], true);
    assert_eq!(json["request"]["required"][0], "session_id");
    assert_eq!(json["response"]["required"][0], "response");
}

#[tokio::test]
async fn legacy_bot_history_endpoint_returns_recent_rows() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, status, submitted_by) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("bot-legacy-history")
    .bind("LegacyHistoryBot")
    .bind("https://example.com/debate")
    .bind("active")
    .bind("owner-user")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO debates (id, topic, status) VALUES (?, ?, ?)")
        .bind("debate-legacy-history")
        .bind("Legacy history topic")
        .bind("complete")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO debate_bots (debate_id, bot_id, pseudonym, role) VALUES (?, ?, ?, ?)")
        .bind("debate-legacy-history")
        .bind("bot-legacy-history")
        .bind("Agent A")
        .bind("skeptic")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO responses (id, debate_id, round_number, bot_id, response_json, abstained, valid) VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("resp-legacy-history")
    .bind("debate-legacy-history")
    .bind(0)
    .bind("bot-legacy-history")
    .bind("legacy response")
    .bind(false)
    .bind(true)
    .execute(&pool)
    .await
    .unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/bots/bot-legacy-history/history")
        .header("authorization", "Bearer owner-user")
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let history = json.as_array().expect("history should be array");
    assert_eq!(history[0]["debate_id"], "debate-legacy-history");
}
