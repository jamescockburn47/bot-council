//! End-to-end integration test for the full 5-round debate protocol.
//!
//! Exercises the production path with 3 text_only + 2 external bots and
//! verifies:
//!   - R1 carry-forward: one text_only bot returns 500 on the first R1
//!     call then effective-abstention text on retry. Its R1 row must show
//!     `fallback_from_round = Some(0)` and `retry_count = 1`.
//!   - Crux selection produces an `analyses` row.
//!   - Divergence rows surface with `crux_shift` populated.
//!   - Synthesis row exists and parses.
//!   - At least one text_only R4 row carries an `extracted` steelman.
//!   - At least one text_only R3 row carries a `frame_rejected` crux
//!     engagement stance.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Prose returned by every text_only bot. Crafted so that the crux quote,
/// challenge quote, position_change quote, crux_engagement quote, and
/// steelman quote are all verbatim substrings (substring-verification
/// lives in every extractor and in crux selection).
const TEXT_ONLY_PROSE: &str = "I challenge the claim that preflight checks prevent 85% of incidents because runtime overhead dominates the cost curve; this is a factual dispute. The strongest opposing argument is that config drift causes most outages, so preflight would catch them early. I reject the framing of the crux entirely — it bakes in an assumption about failure distribution that the debate has not validated. My position has not changed: runtime checks remain premature optimisation.";

/// Mount a text_only endpoint. When `trigger_r1_abstention`, R1 first call
/// 500s and the retry returns effective-abstention text so dispatch carries
/// R0 forward. Trigger mocks MUST mount first — wiremock evaluates in
/// registration order, first-to-match wins.
async fn mount_text_only_bot(server: &MockServer, prefix: &str, trigger_r1_abstention: bool) {
    if trigger_r1_abstention {
        // Real R1 prompt carries "Here are the initial positions from all
        // participants" (see `prompts::round1_prompt`). 500 once then exhausts.
        Mock::given(method("POST"))
            .and(path(prefix.to_string()))
            .and(body_string_contains(
                "Here are the initial positions from all participants",
            ))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .mount(server)
            .await;
        // Simplified retry prompt from `dispatch::simplified_retry_prompt(_, 1)`
        // carries "Current round: 1". Return the canonical abstention marker.
        Mock::given(method("POST"))
            .and(path(prefix.to_string()))
            .and(body_string_contains("Current round: 1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "text": "I was unable to formulate a response."
            })))
            .up_to_n_times(1)
            .mount(server)
            .await;
    }
    // Default: return valid prose for any other request (intro probe, 5
    // smoke probes, preflight, R0/R2/R3/R4).
    Mock::given(method("POST"))
        .and(path(prefix.to_string()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "text": TEXT_ONLY_PROSE
        })))
        .mount(server)
        .await;
}

/// Mount an external bot endpoint. Mirrors the round-shaped mock pattern
/// in `tests/text_only_bot_flow.rs::mock_external_bot`.
async fn mount_external_bot(server: &MockServer, prefix: &str, label: &str) {
    let bot_path = format!("{prefix}/debate");
    Mock::given(method("POST"))
        .and(path(bot_path.clone()))
        .and(body_string_contains("\"scoring\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"scores": []})))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(bot_path.clone()))
        .and(body_string_contains("\"round\":2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": format!("{label} r2"),
            "confidence": 65,
            "challenge": {
                "claim_targeted": "preflight checks prevent 85% of incidents",
                "counter_evidence": "runtime overhead dominates",
                "type": "factual"
            }
        })))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(bot_path.clone()))
        .and(body_string_contains("\"round\":4"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": format!("{label} r4"),
            "confidence": 75,
            "position_change": {
                "changed": false,
                "from_summary": "opening",
                "to_summary": "opening",
                "reason": "no new evidence"
            }
        })))
        .mount(server)
        .await;
    for (round, confidence) in [(0i64, None), (1, Some(70)), (3, Some(60))] {
        let mut body = json!({"response": format!("{label} r{round}")});
        if let Some(c) = confidence {
            body["confidence"] = json!(c);
        }
        Mock::given(method("POST"))
            .and(path(bot_path.clone()))
            .and(body_string_contains(format!("\"round\":{round}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }
}

/// Set up a MiniMax mock for every LLM call the 5-round flow makes.
/// Each mock is anchored on a phrase unique to its prompt, avoiding
/// cross-mock shadowing.
async fn mount_minimax(server: &MockServer) {
    let mounts: [(&str, Value); 7] = [
        // Crux selection. Returns a verbatim substring of TEXT_ONLY_PROSE
        // so the selector's quote verification passes.
        (
            "Identify the single claim",
            json!({
                "claim": "preflight checks prevent 85% of incidents",
                "source_pseudonym": "Agent A",
                "source_quote": "preflight checks prevent 85% of incidents"
            }),
        ),
        // R3 crux-engagement extractor — forces `frame_rejected` stance.
        (
            "engagement with the R3 crux",
            json!({
                "engagement_stance": "frame_rejected",
                "reasoning_quote": "I reject the framing of the crux entirely"
            }),
        ),
        // R4 steelman extractor.
        (
            "Extract the bot's steelman",
            json!({
                "steelman": "config drift causes most outages so preflight catches them early",
                "source_quote": "config drift causes most outages"
            }),
        ),
        // R2 challenge extractor — matches on `factual|logical|premise`
        // which only appears in that schema_spec.
        (
            "factual|logical|premise",
            json!({
                "extracted": true,
                "fields": {
                    "claim_targeted": {"value": "preflight checks prevent 85% of incidents", "quote": "preflight checks prevent 85% of incidents"},
                    "counter_evidence": {"value": "runtime overhead dominates", "quote": "runtime overhead dominates the cost curve"},
                    "type": {"value": "factual", "quote": "factual dispute"}
                }
            }),
        ),
        // Divergence analyser — returns `crux_shift` field populated.
        (
            "Compare these two positions",
            json!({
                "shifted": false,
                "magnitude": "none",
                "what_changed": "no change",
                "justification_adequate": true,
                "flags": [],
                "crux_shift": "frame_rejected"
            }),
        ),
        // Final synthesis — `is_crux` only appears in
        // `build_synthesis_prompt`'s OUTPUT SCHEMA block (not in any
        // extractor / validator / divergence / pairing prompt). The mock
        // must carry a populated issue: an empty issues array would trip
        // run_synthesis's retry-on-empty loop.
        (
            "is_crux",
            json!({
                "topic": "runtime preflight checks",
                "headline": "Council split on preflight value.",
                "executive_summary": "The debate considered whether preflight checks are worth the cost. Participants weighed incident-reduction claims against deployment overhead. On balance the reform case drew more support than full removal. The central unresolved issue was the correct evidentiary threshold for worth.",
                "issues": [{
                    "issue": "Whether preflight checks are worth their cost",
                    "headline": "Preflight cost-benefit threshold",
                    "is_crux": false,
                    "status": "split",
                    "positions": [{
                        "stance": "Preflight checks reduce incident volume when well-scoped.",
                        "headline": "Scoped preflight reduces incidents",
                        "bots": ["Agent A"],
                        "best_argument": "Incident data supports scoping [Agent A, Round 1]",
                        "evidence": "Agent A, Round 1 cited incident-reduction data.",
                        "final_confidence": 70,
                        "frame_rejection": false
                    }],
                    "movement": []
                }],
                "meta_observations": "Conclusion: test synthesis."
            }),
        ),
        // R4 position_change extractor. Anchored on BOTH the extractor
        // opener AND `from_summary` so it cannot shadow the synthesis mock
        // (which also references `from_summary`). Listed last so if another
        // matcher already caught the request, this one is just a safety net.
        (
            "structured-extraction assistant",
            json!({
                "extracted": true,
                "fields": {
                    "changed": {"value": false, "quote": "My position has not changed"},
                    "from_summary": {"value": "runtime checks remain premature optimisation", "quote": "runtime checks remain premature optimisation"},
                    "to_summary": {"value": "runtime checks remain premature optimisation", "quote": "runtime checks remain premature optimisation"},
                    "reason": {"value": "no new evidence", "quote": "My position has not changed"}
                }
            }),
        ),
    ];
    for (phrase, payload) in mounts {
        let content_body = json!({
            "choices": [{"message": {"content": payload.to_string()}}]
        });
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains(phrase))
            .respond_with(ResponseTemplate::new(200).set_body_json(content_body))
            .mount(server)
            .await;
    }
}

/// Poll the debate detail endpoint until complete or timeout.
async fn wait_for_terminal(app: &axum::Router, debate_id: &str, timeout_secs: u64) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let req = common::admin_auth(
            Request::builder()
                .method("GET")
                .uri(format!("/debates/{debate_id}")),
        )
        .body(Body::empty())
        .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        let status = json["status"].as_str().unwrap_or("").to_string();
        if status == "complete" || status == "failed" {
            return status;
        }
        if std::time::Instant::now() >= deadline {
            panic!("debate {debate_id} did not terminate in {timeout_secs}s; last={status}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

/// Register a bot through admin POST (auto-active, bypasses approval smoke).
async fn register_bot(app: &axum::Router, name: &str, endpoint: &str, text_only: bool) -> String {
    let mut body = json!({
        "name": name,
        "endpoint_url": endpoint,
        "token": "t",
    });
    if text_only {
        body["bot_kind"] = json!("text_only");
    }
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/bots")
            .header("content-type", "application/json"),
    )
    .body(Body::from(body.to_string()))
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED, "register {name}");
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let j: Value = serde_json::from_slice(&bytes).unwrap();
    j["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn five_round_flow_with_abstention_crux_and_steelman() {
    let bot_server = MockServer::start().await;
    mount_text_only_bot(&bot_server, "/bot1", false).await;
    mount_text_only_bot(&bot_server, "/bot2", false).await;
    mount_text_only_bot(&bot_server, "/bot3", true).await;
    mount_external_bot(&bot_server, "/bot4", "bot-d").await;
    mount_external_bot(&bot_server, "/bot5", "bot-e").await;

    let minimax = MockServer::start().await;
    mount_minimax(&minimax).await;

    let (app, pool) = common::test_app_with_minimax_url(&minimax.uri()).await;
    let base = bot_server.uri();
    let id1 = register_bot(&app, "Bot1", &format!("{base}/bot1"), true).await;
    let id2 = register_bot(&app, "Bot2", &format!("{base}/bot2"), true).await;
    let id3 = register_bot(&app, "Bot3", &format!("{base}/bot3"), true).await;
    let id4 = register_bot(&app, "Bot4", &format!("{base}/bot4/debate"), false).await;
    let id5 = register_bot(&app, "Bot5", &format!("{base}/bot5/debate"), false).await;

    let debate_body = json!({
        "topic": "Are runtime preflight checks worth the overhead?",
        "bot_ids": [id1, id2, id3.clone(), id4, id5]
    });
    let req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/debates")
            .header("content-type", "application/json"),
    )
    .body(Body::from(debate_body.to_string()))
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let debate_id = serde_json::from_slice::<Value>(&bytes).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let terminal = wait_for_terminal(&app, &debate_id, 45).await;
    assert_eq!(terminal, "complete", "debate must complete; got {terminal}");

    // Assertion 1: Bot3 R1 carry-forward from R0.
    let (_resp, retry_count, fallback_from): (String, i64, Option<i64>) = sqlx::query_as(
        "SELECT response_json, retry_count, fallback_from_round FROM responses \
         WHERE debate_id = ? AND bot_id = ? AND round_number = 1",
    )
    .bind(&debate_id)
    .bind(&id3)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        fallback_from,
        Some(0),
        "bot3 R1 must be carried-forward from R0"
    );
    assert_eq!(retry_count, 1, "bot3 R1 must record one retry");

    // Assertion 2: crux_selection analysis row.
    let crux_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM analyses WHERE debate_id = ? AND analysis_type = 'crux_selection'",
    )
    .bind(&debate_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(crux_rows, 1, "expected one crux_selection row");

    // Assertion 3: divergence rows with crux_shift.
    let div_rows: Vec<(String,)> = sqlx::query_as(
        "SELECT result_json FROM analyses WHERE debate_id = ? AND analysis_type = 'divergence'",
    )
    .bind(&debate_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!div_rows.is_empty(), "divergence rows must exist");
    assert!(
        div_rows.iter().any(|(j,)| j.contains("crux_shift")),
        "at least one divergence row must carry crux_shift"
    );

    // Assertion 4: synthesis row parses cleanly.
    let (synth_json,): (String,) =
        sqlx::query_as("SELECT output_json FROM syntheses WHERE debate_id = ?")
            .bind(&debate_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let synth: Value = serde_json::from_str(&synth_json).expect("synthesis JSON must parse");
    assert!(synth.get("topic").is_some(), "synthesis must contain topic");

    // Assertion 5: at least one text_only R4 steelman was extracted.
    let r4_meta: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT extraction_metadata FROM responses \
         WHERE debate_id = ? AND round_number = 4 AND extraction_metadata IS NOT NULL",
    )
    .bind(&debate_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        r4_meta.iter().any(|(m,)| m
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .map(|v| v["steelman"]["source"].as_str() == Some("extracted"))
            .unwrap_or(false)),
        "at least one text_only R4 steelman must have source=extracted"
    );

    // Assertion 6: at least one text_only R3 crux_engagement is frame_rejected.
    let r3_meta: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT extraction_metadata FROM responses \
         WHERE debate_id = ? AND round_number = 3 AND extraction_metadata IS NOT NULL",
    )
    .bind(&debate_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(
        r3_meta.iter().any(|(m,)| m
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .map(|v| v["crux_engagement"]["stance"].as_str() == Some("frame_rejected"))
            .unwrap_or(false)),
        "at least one text_only R3 row must have crux_engagement stance=frame_rejected"
    );
}
