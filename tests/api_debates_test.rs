mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mount round-specific /debate mocks that satisfy the 5-round approval
/// smoke gauntlet: round 2 needs `challenge`, round 4 needs
/// `position_change`, rounds 1-4 need integer `confidence`.
async fn mount_five_round_mocks(server: &MockServer, body_suffix: &str) {
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("round 0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": format!("r0 {body_suffix}")
        })))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("round 1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": format!("r1 {body_suffix}"), "confidence": 70
        })))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("round 2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": format!("r2 {body_suffix}"),
            "confidence": 65,
            "challenge": {
                "claim_targeted": "stub",
                "counter_evidence": "stub",
                "type": "factual"
            }
        })))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("round 3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": format!("r3 {body_suffix}"), "confidence": 60
        })))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/debate"))
        .and(body_string_contains("round 4"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": format!("r4 {body_suffix}"),
            "confidence": 75,
            "position_change": {
                "changed": false,
                "from_summary": "stub",
                "to_summary": "stub",
                "reason": "stub"
            }
        })))
        .mount(server)
        .await;
}

async fn seed_bots(app: &mut axum::Router) -> (Vec<String>, Vec<MockServer>) {
    let mut ids = Vec::new();
    let mut servers = Vec::new();
    for i in 0..3 {
        let server = MockServer::start().await;
        mount_five_round_mocks(&server, &format!("bot {i}")).await;
        let body = json!({
            "name": format!("Bot{}", i),
            "endpoint_url": format!("{}/debate", server.uri()),
            "token": format!("token{}", i)
        });
        let req = common::admin_auth(
            Request::builder()
                .method("POST")
                .uri("/bots")
                .header("content-type", "application/json"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        ids.push(json["id"].as_str().unwrap().to_string());
        servers.push(server);
    }
    (ids, servers)
}

#[tokio::test]
async fn test_create_debate_returns_201() {
    let (mut app, _pool) = common::test_app().await;
    let (bot_ids, _servers) = seed_bots(&mut app).await;
    let body = json!({"topic": "Should AI-generated evidence be admissible in court?", "bot_ids": bot_ids});
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/debates")
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
    assert_eq!(
        json["topic"],
        "Should AI-generated evidence be admissible in court?"
    );
    assert_eq!(json["bots"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_create_debate_rejects_insufficient_bots() {
    let (app, _pool) = common::test_app().await;
    let body = json!({"topic": "Test topic", "bot_ids": []});
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/debates")
            .header("content-type", "application/json"),
    )
    .body(Body::from(serde_json::to_string(&body).unwrap()))
    .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_debate_not_found() {
    let (app, _pool) = common::test_app().await;
    let req = common::admin_auth(Request::builder().uri("/debates/nonexistent"))
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

#[tokio::test]
async fn create_debate_skips_unreachable_bots_when_quorum_still_met() {
    let (mut app, _pool) = common::test_app().await;
    let (mut bot_ids, _servers) = seed_bots(&mut app).await;

    // Add one unreachable bot and include it in the debate set.
    let bad = json!({
        "name": "OfflineBot",
        "endpoint_url": "http://127.0.0.1:9/debate",
        "token": "offline-token"
    });
    let bad_req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/bots")
            .header("content-type", "application/json"),
    )
    .body(Body::from(serde_json::to_string(&bad).unwrap()))
    .unwrap();
    let bad_res = app.clone().oneshot(bad_req).await.unwrap();
    assert_eq!(bad_res.status(), StatusCode::CREATED);
    let bad_body = axum::body::to_bytes(bad_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let bad_json: Value = serde_json::from_slice(&bad_body).unwrap();
    bot_ids.push(bad_json["id"].as_str().unwrap().to_string());

    let body = json!({
        "topic": "Preflight failure case",
        "bot_ids": bot_ids
    });
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/debates")
            .header("content-type", "application/json"),
    )
    .body(Body::from(serde_json::to_string(&body).unwrap()))
    .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    let assigned = payload["bots"].as_array().expect("bots array");
    assert_eq!(assigned.len(), 3, "expected offline bot to be excluded");
    assert!(
        assigned.iter().all(|b| b["bot_name"] != "OfflineBot"),
        "offline bot should not be assigned"
    );
}

#[tokio::test]
async fn create_debate_rejects_when_preflight_leaves_fewer_than_three_bots() {
    let (app, _pool) = common::test_app().await;
    let mut selected_bot_ids = Vec::new();

    for i in 0..3 {
        let body = json!({
            "name": format!("OfflineBot{i}"),
            "endpoint_url": "http://127.0.0.1:9/debate",
            "token": format!("offline-token-{i}")
        });
        let req = common::admin_auth(
            Request::builder()
                .method("POST")
                .uri("/bots")
                .header("content-type", "application/json"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&bytes).unwrap();
        selected_bot_ids.push(payload["id"].as_str().unwrap().to_string());
    }

    let body = json!({
        "topic": "Preflight quorum failure case",
        "bot_ids": selected_bot_ids
    });
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/debates")
            .header("content-type", "application/json"),
    )
    .body(Body::from(serde_json::to_string(&body).unwrap()))
    .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    let msg = payload["error"].as_str().unwrap_or_default();
    assert!(
        msg.contains("bot preflight failed"),
        "unexpected message: {msg}"
    );
    assert!(msg.contains("need at least 3"), "unexpected message: {msg}");
}

#[tokio::test]
async fn get_debate_resolves_bot_name_for_inactive_bot() {
    let (app, pool) = common::test_app().await;
    sqlx::query("INSERT INTO bots (id, name, endpoint_url, status) VALUES (?, ?, ?, ?)")
        .bind("bot-inactive-name")
        .bind("ArchivedBot")
        .bind("https://example.com/debate")
        .bind("inactive")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO debates (id, topic, status) VALUES (?, ?, ?)")
        .bind("debate-name-resolution")
        .bind("name resolution")
        .bind("created")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO debate_bots (debate_id, bot_id, pseudonym, role) VALUES (?, ?, ?, ?)")
        .bind("debate-name-resolution")
        .bind("bot-inactive-name")
        .bind("Agent A")
        .bind("skeptic")
        .execute(&pool)
        .await
        .unwrap();

    let req = common::admin_auth(
        Request::builder()
            .method("GET")
            .uri("/debates/debate-name-resolution"),
    )
    .body(Body::empty())
    .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["bots"][0]["bot_name"], "ArchivedBot");
}

#[tokio::test]
async fn list_debates_excludes_operator_test_topics() {
    let (app, pool) = common::test_app().await;
    sqlx::query("INSERT INTO debates (id, topic, status) VALUES (?, ?, ?)")
        .bind("debate-visible")
        .bind("Should AI-generated evidence be admissible in court?")
        .bind("complete")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO debates (id, topic, status) VALUES (?, ?, ?)")
        .bind("debate-hidden")
        .bind("Quickfire readiness check for TestBot")
        .bind("complete")
        .execute(&pool)
        .await
        .unwrap();

    let req = common::admin_auth(Request::builder().method("GET").uri("/debates?limit=20"))
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let topics: Vec<String> = json
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["topic"].as_str().map(str::to_string))
        .collect();
    assert!(
        topics
            .iter()
            .any(|t| t == "Should AI-generated evidence be admissible in court?")
    );
    assert!(
        !topics
            .iter()
            .any(|t| t.contains("Quickfire readiness check"))
    );
}

#[tokio::test]
async fn list_debates_test_view_includes_only_operator_test_topics() {
    let (app, pool) = common::test_app().await;
    sqlx::query("INSERT INTO debates (id, topic, status) VALUES (?, ?, ?)")
        .bind("debate-main")
        .bind("Should AI-generated evidence be admissible in court?")
        .bind("complete")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO debates (id, topic, status) VALUES (?, ?, ?)")
        .bind("debate-test")
        .bind("Quickfire readiness check for TestBot")
        .bind("complete")
        .execute(&pool)
        .await
        .unwrap();

    let req = common::admin_auth(
        Request::builder()
            .method("GET")
            .uri("/debates?test=true&limit=20"),
    )
    .body(Body::empty())
    .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let topics: Vec<String> = json
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["topic"].as_str().map(str::to_string))
        .collect();
    assert!(
        topics
            .iter()
            .any(|t| t.contains("Quickfire readiness check"))
    );
    assert!(
        !topics
            .iter()
            .any(|t| t == "Should AI-generated evidence be admissible in court?")
    );
}

#[tokio::test]
async fn debate_creation_does_not_reject_null_token_bot_at_preflight() {
    // Token is optional under the unified contract: a NULL-token bot passes
    // the preflight token check. The smoke test still runs, and the
    // `example.invalid` endpoint will fail for connectivity, but the
    // failure message must NOT cite a missing token as the reason.
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, status, token_ciphertext, bot_kind) \
         VALUES ('nulltok', 'NoToken', 'http://example.invalid/debate', 'active', NULL, 'external')",
    ).execute(&pool).await.unwrap();
    for i in 0..2 {
        sqlx::query(&format!(
            "INSERT INTO bots (id, name, endpoint_url, status, token_ciphertext, bot_kind) \
             VALUES ('real{i}', 'Real{i}', 'http://example.invalid/debate', 'active', x'DEADBEEF', 'external')"
        )).execute(&pool).await.unwrap();
    }
    let body = serde_json::json!({
        "topic": "smoke",
        "bot_ids": ["nulltok", "real0", "real1"]
    });
    let req = common::admin_auth(
        axum::http::Request::builder()
            .method("POST")
            .uri("/debates")
            .header("content-type", "application/json"),
    )
    .body(axum::body::Body::from(body.to_string()))
    .unwrap();
    let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
    assert_eq!(resp.status(), 400);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        !body_str.contains("no encrypted token"),
        "token-null must not be a preflight failure reason: {body_str}"
    );
}

/// Regression: when one of the selected bots is unreachable, preflight must
/// exclude it (not hang waiting for the 5-round gauntlet). Before the fix,
/// `create_debate` ran the full smoke gauntlet per bot; a single slow bot
/// could push the request past Cloudflare's 100s edge timeout and surface
/// as HTTP 524 to the admin. The lightweight `preflight_probe_bot` keeps
/// total preflight well under the outer 45s budget.
#[tokio::test]
async fn test_create_debate_excludes_unreachable_bot() {
    let (mut app, _pool) = common::test_app().await;
    let (mut bot_ids, _servers) = seed_bots(&mut app).await;

    // Bind an ephemeral port then drop the listener — the port is guaranteed
    // to refuse connections for the duration of the test, so the preflight
    // probe will fast-fail with ECONNREFUSED rather than sitting on the
    // 25s timeout.
    let dead_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    };
    let dead_url = format!("http://127.0.0.1:{dead_port}/debate");

    let body = json!({ "name": "DeadBot", "endpoint_url": dead_url, "token": "dead" });
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/bots")
            .header("content-type", "application/json"),
    )
    .body(Body::from(serde_json::to_string(&body).unwrap()))
    .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json_body: Value = serde_json::from_slice(&body_bytes).unwrap();
    let dead_id = json_body["id"].as_str().unwrap().to_string();
    bot_ids.push(dead_id.clone());

    let body = json!({
        "topic": "Preflight excludes unreachable bots cleanly",
        "bot_ids": bot_ids,
    });
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/debates")
            .header("content-type", "application/json"),
    )
    .body(Body::from(serde_json::to_string(&body).unwrap()))
    .unwrap();
    let started = std::time::Instant::now();
    let response = app.oneshot(req).await.unwrap();
    let elapsed = started.elapsed();

    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "preflight took {elapsed:?}; expected <30s for 3 healthy bots + 1 fast-refused connection"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json_body: Value = serde_json::from_slice(&body).unwrap();
    let bots = json_body["bots"].as_array().unwrap();
    assert_eq!(bots.len(), 3, "dead bot must be excluded, found bots: {bots:?}");
    let bot_id_set: std::collections::HashSet<_> =
        bots.iter().map(|b| b["bot_id"].as_str().unwrap()).collect();
    assert!(
        !bot_id_set.contains(dead_id.as_str()),
        "dead bot id {dead_id} should not be in the debate roster"
    );
}
