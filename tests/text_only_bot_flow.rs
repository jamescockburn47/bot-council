//! End-to-end test: register a text_only bot via the admin API, run a full
//! debate with 5 rounds, and verify that structured fields were extracted
//! from the bot's prose at rounds 2 + 4 with source-quote provenance
//! persisted into `responses.extraction_metadata`.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{Value, json};
use tower::ServiceExt;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Prose the text-only bot returns on every call. Crafted so MiniMax can
/// extract valid R2 challenge + R4 position_change structures whose source
/// quotes are verbatim substrings of this string.
const TEXT_ONLY_BOT_PROSE: &str = "I challenge the claim that preflight checks help because evidence that 85% of incidents stem from config drift contradicts it; this is a factual dispute. My position has not changed — I still think runtime checks are premature optimisation, and the evidence still points toward config review as the better lever.";

/// Stand up a text_only bot endpoint. Returns the same prose on every POST
/// (introduction probe, 5 smoke probes, 5 debate rounds, scoring round).
async fn mock_text_only_bot() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "text": TEXT_ONLY_BOT_PROSE
        })))
        .mount(&server)
        .await;
    server
}

/// Stand up an external-mode bot endpoint with round-shaped responses.
///
/// Smoke-test and real-debate request bodies both serialise `round` as a
/// JSON integer field, so matching on `"round":N` captures BOTH the
/// approval smoke gauntlet and the 5 debate rounds. Peer scoring sends
/// `round: "scoring"` as a string, matched separately.
async fn mock_external_bot(label: &str) -> MockServer {
    let server = MockServer::start().await;
    // Peer scoring — match before rounds to avoid any ambiguity. Scoring
    // sends `round: "scoring"` (string), not an integer.
    Mock::given(method("POST"))
        .and(body_string_contains("\"scoring\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "scores": [
                {"pseudonym": "Agent B", "reasoning_quality": 7,
                 "factual_grounding": 7, "overall": 7, "reasoning": "solid"},
                {"pseudonym": "Agent C", "reasoning_quality": 6,
                 "factual_grounding": 6, "overall": 6, "reasoning": "ok"}
            ]
        })))
        .mount(&server)
        .await;

    // R2 — structured rebuttal with a mandatory `challenge`.
    Mock::given(method("POST"))
        .and(body_string_contains("\"round\":2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": format!("{label} r2 rebuttal"),
            "confidence": 65,
            "challenge": {
                "claim_targeted": "the opposing proposal is sound",
                "counter_evidence": "data shows it fails 40% of the time",
                "type": "factual"
            }
        })))
        .mount(&server)
        .await;

    // R4 — final position with a mandatory `position_change`.
    Mock::given(method("POST"))
        .and(body_string_contains("\"round\":4"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response": format!("{label} r4 final"),
            "confidence": 75,
            "position_change": {
                "changed": false,
                "from_summary": "opening stance",
                "to_summary": "opening stance",
                "reason": "no new evidence changed the position"
            }
        })))
        .mount(&server)
        .await;

    // R0, R1, R3 — response-only schema (confidence only on R1/R3).
    for (round, confidence) in [(0i64, None), (1, Some(70)), (3, Some(60))] {
        let mut body = json!({"response": format!("{label} r{round} ok")});
        if let Some(c) = confidence {
            body["confidence"] = json!(c);
        }
        Mock::given(method("POST"))
            .and(body_string_contains(format!("\"round\":{round}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
    }
    server
}

/// MiniMax mock covering the whole debate's LLM calls: extractor (R2
/// challenge + R4 position_change), challenge validation (external bots
/// with a structured challenge), divergence analysis, final synthesis.
///
/// Matchers are registered most-specific-first. wiremock picks the first
/// mock whose matchers all pass, so the shorter keywords in later mocks
/// will not fire when an earlier matcher already captured the request.
async fn mock_minimax_server() -> MockServer {
    let server = MockServer::start().await;

    // R2 extractor — identified by the challenge-shaped schema spec.
    let challenge_extraction = json!({
        "choices": [{"message": {"content": r#"{
            "extracted": true,
            "fields": {
                "claim_targeted": {"value": "preflight checks help", "quote": "the claim that preflight checks help"},
                "counter_evidence": {"value": "85% of incidents stem from config drift", "quote": "evidence that 85% of incidents stem from config drift"},
                "type": {"value": "factual", "quote": "factual dispute"}
            }
        }"#}}]
    });
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("factual|logical|premise"))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_extraction))
        .mount(&server)
        .await;

    // R4 extractor — identified by the position_change schema spec.
    // The `from_summary` field name alone is ambiguous: it also appears in
    // the synthesis prompt (PrecomputedData.position_changes serialises
    // `from_summary` into <structural-data>), so a bare body_string_contains
    // would shadow the synthesis mock below. Anchor on the extractor-only
    // opener "structured-extraction assistant" to disambiguate.
    let position_extraction = json!({
        "choices": [{"message": {"content": r#"{
            "extracted": true,
            "fields": {
                "changed": {"value": false, "quote": "My position has not changed"},
                "from_summary": {"value": "runtime checks are premature optimisation", "quote": "runtime checks are premature optimisation"},
                "to_summary": {"value": "runtime checks are premature optimisation", "quote": "runtime checks are premature optimisation"},
                "reason": {"value": "evidence points to config review", "quote": "the evidence still points toward config review as the better lever"}
            }
        }"#}}]
    });
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("structured-extraction assistant"))
        .and(body_string_contains("from_summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(position_extraction))
        .mount(&server)
        .await;

    // Challenge validation for external bots' R2 challenges — identified
    // by the `validate_challenge` prompt wording.
    let challenge_validation = json!({
        "choices": [{"message": {"content": r#"{"valid": true, "reason": "ok"}"#}}]
    });
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(
            "specific factual claim, logical objection",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(challenge_validation))
        .mount(&server)
        .await;

    // R3 pairing — matched on the unique pairing prompt opener. The
    // three pseudonyms assigned to a 3-bot debate are Agent A, B, C.
    let pairing_result = json!({
        "choices": [{"message": {"content": r#"{
            "pair_1": ["Agent A", "Agent B"],
            "pair_2": ["Agent A", "Agent C"],
            "third_joins": "pair_1",
            "third": "Agent C"
        }"#}}]
    });
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains(
            "identify the two pairs of participants",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(pairing_result))
        .mount(&server)
        .await;

    // Divergence analysis — uses the unique divergence prompt opener so it
    // doesn't collide with the synthesis prompt (which also mentions
    // "shifted" and "justification_adequate" in its schema description).
    let divergence_result = json!({
        "choices": [{"message": {"content": r#"{
            "shifted": false,
            "magnitude": "none",
            "what_changed": "no change",
            "justification_adequate": true,
            "flags": []
        }"#}}]
    });
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("Compare these two positions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(divergence_result))
        .mount(&server)
        .await;

    // Final synthesis — matched by a phrase unique to build_synthesis_prompt.
    // We previously matched on "consensus_points" and (before that) shared
    // "from_summary" with the R4 extractor prompt. The R4 extractor prompt
    // ALSO contains "from_summary" (it's a required field in the
    // PositionChange schema spec), and wiremock's insertion-order fallback
    // was letting the R4 extractor mock capture synthesis calls — synthesis
    // then silently fell through to salvage_loose_output because the
    // extractor's JSON shape doesn't fit SynthesisOutput. `minority_positions`
    // appears only in the synthesis prompt's OUTPUT SCHEMA block and nowhere
    // in any extractor / validator / divergence / pairing prompt.
    let synthesis_output = json!({
        "choices": [{"message": {"content": json!({
            "topic": "preflight checks",
            "consensus_points": [],
            "live_disagreements": [],
            "flagged_capitulations": [],
            "minority_positions": [],
            "confidence_trajectories": {},
            "meta_observations": "test synthesis"
        }).to_string()}}]
    });
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_string_contains("minority_positions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(synthesis_output))
        .expect(1)
        .mount(&server)
        .await;

    server
}

/// Poll the debate detail endpoint until status is `complete`, `failed`,
/// or the timeout elapses. Returns the final status string.
async fn wait_for_debate_terminal(
    app: &axum::Router,
    debate_id: &str,
    timeout_secs: u64,
) -> String {
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
            panic!(
                "debate {debate_id} did not terminate within {timeout_secs}s; last status = {status}"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

#[tokio::test]
async fn text_only_bot_completes_debate_with_extracted_fields() {
    // 1. Mock servers: MiniMax + one text_only bot + two external peers.
    let minimax = mock_minimax_server().await;
    let text_only = mock_text_only_bot().await;
    let peer_a = mock_external_bot("bot-a").await;
    let peer_b = mock_external_bot("bot-b").await;

    // 2. Build the test app with analysis + synthesis routed at MiniMax mock.
    let (app, pool) = common::test_app_with_minimax_url(&minimax.uri()).await;

    // 3. Submit the text_only bot as a PARTICIPANT so it lands in `pending`
    //    status and must be explicitly approved (the approval path is what
    //    fires the introduction probe + relaxed 5-round smoke).
    let submit_body = json!({
        "name": "TextOnlyBot",
        "endpoint_url": text_only.uri(),
        "token": "text-only-token",
        "bot_kind": "text_only",
        "description": "Integration test bot"
    });
    let submit_req = Request::builder()
        .method("POST")
        .uri("/bots")
        .header("content-type", "application/json")
        .header("authorization", "Bearer participant-user-1")
        .body(Body::from(serde_json::to_string(&submit_body).unwrap()))
        .unwrap();
    let submit_res = app.clone().oneshot(submit_req).await.unwrap();
    assert_eq!(submit_res.status(), StatusCode::CREATED);
    let submit_bytes = axum::body::to_bytes(submit_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let submit_json: Value = serde_json::from_slice(&submit_bytes).unwrap();
    let text_only_id = submit_json["id"].as_str().unwrap().to_string();
    assert_eq!(submit_json["status"], "pending");

    // 4. Approve the text-only bot — this fires the introduction probe AND
    //    the 5-round text-only smoke gauntlet.
    let approve_req = common::admin_auth(
        Request::builder()
            .method("PATCH")
            .uri(format!("/bots/{text_only_id}/approve")),
    )
    .body(Body::empty())
    .unwrap();
    let approve_res = app.clone().oneshot(approve_req).await.unwrap();
    assert_eq!(
        approve_res.status(),
        StatusCode::OK,
        "approve failed for text-only bot"
    );

    // 5. Verify bot_kind + introduction landed in the DB (the GET /bots/{id}
    //    response DTO does not currently surface these fields; the canonical
    //    source of truth is the row, so assert against the row).
    let (bot_kind, introduction): (String, Option<String>) =
        sqlx::query_as("SELECT bot_kind, introduction FROM bots WHERE id = ?")
            .bind(&text_only_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(bot_kind, "text_only");
    assert!(
        introduction.is_some(),
        "introduction must be captured by the approval probe"
    );
    let intro = introduction.unwrap();
    assert!(!intro.trim().is_empty(), "introduction must be non-empty");

    // 6. Register two external peer bots (admin auth — auto-active).
    let mut peer_ids = Vec::new();
    for (name, endpoint) in [
        ("PeerA", format!("{}/debate", peer_a.uri())),
        ("PeerB", format!("{}/debate", peer_b.uri())),
    ] {
        let body = json!({
            "name": name,
            "endpoint_url": endpoint,
            "token": format!("token-{name}")
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
        let j: Value = serde_json::from_slice(&bytes).unwrap();
        peer_ids.push(j["id"].as_str().unwrap().to_string());
    }

    // 7. Trigger a real debate that includes the text-only bot + the two peers.
    let mut bot_ids = vec![text_only_id.clone()];
    bot_ids.extend(peer_ids.iter().cloned());
    let debate_body = json!({
        "topic": "Are runtime preflight checks worth their cost?",
        "bot_ids": bot_ids
    });
    let debate_req = common::admin_auth(
        Request::builder()
            .method("POST")
            .uri("/debates")
            .header("content-type", "application/json"),
    )
    .body(Body::from(serde_json::to_string(&debate_body).unwrap()))
    .unwrap();
    let debate_res = app.clone().oneshot(debate_req).await.unwrap();
    assert_eq!(debate_res.status(), StatusCode::CREATED);
    let debate_bytes = axum::body::to_bytes(debate_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let debate_json: Value = serde_json::from_slice(&debate_bytes).unwrap();
    let debate_id = debate_json["id"].as_str().unwrap().to_string();

    // 8. Poll until the debate reaches a terminal state. The full 5-round
    //    flow + analysis + synthesis must finish inside the timeout.
    let terminal = wait_for_debate_terminal(&app, &debate_id, 30).await;
    assert_eq!(
        terminal, "complete",
        "expected debate to complete, got {terminal}"
    );

    // 9. Transcript: Round 2 must show a populated `challenge` field for the
    //    text-only bot (proof that extraction ran and patched the response).
    //    Round 4 must show a populated `position_change`.
    let transcript_req = common::admin_auth(
        Request::builder()
            .method("GET")
            .uri(format!("/debates/{debate_id}/transcript")),
    )
    .body(Body::empty())
    .unwrap();
    let transcript_res = app.clone().oneshot(transcript_req).await.unwrap();
    assert_eq!(transcript_res.status(), StatusCode::OK);
    let transcript_bytes = axum::body::to_bytes(transcript_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let transcript: Value = serde_json::from_slice(&transcript_bytes).unwrap();

    // Find the pseudonym the text-only bot was assigned.
    let (pseudonym,): (String,) =
        sqlx::query_as("SELECT pseudonym FROM debate_bots WHERE debate_id = ? AND bot_id = ?")
            .bind(&debate_id)
            .bind(&text_only_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    let rounds = transcript["rounds"]
        .as_array()
        .expect("transcript rounds array");
    let round2 = rounds
        .iter()
        .find(|r| r["round_number"] == 2)
        .expect("round 2 present");
    let r2_entry = round2["responses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["pseudonym"] == pseudonym)
        .expect("text-only bot entry in round 2");
    let r2_challenge = &r2_entry["challenge"];
    assert!(
        !r2_challenge.is_null(),
        "round 2 challenge must be populated via extraction for text-only bot; entry = {r2_entry}"
    );
    assert_eq!(
        r2_challenge["claim_targeted"], "preflight checks help",
        "extracted claim_targeted mismatch"
    );

    let round4 = rounds
        .iter()
        .find(|r| r["round_number"] == 4)
        .expect("round 4 present");
    let r4_entry = round4["responses"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["pseudonym"] == pseudonym)
        .expect("text-only bot entry in round 4");
    let r4_pc = &r4_entry["position_change"];
    assert!(
        !r4_pc.is_null(),
        "round 4 position_change must be populated via extraction for text-only bot; entry = {r4_entry}"
    );
    assert_eq!(r4_pc["changed"], false);

    // 10. Provenance: extraction_metadata must record source="extracted" and
    //     a non-empty quote for the text-only bot's R2 + R4 rows.
    let (r2_meta,): (Option<String>,) = sqlx::query_as(
        "SELECT extraction_metadata FROM responses \
         WHERE debate_id = ? AND bot_id = ? AND round_number = 2",
    )
    .bind(&debate_id)
    .bind(&text_only_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let r2_meta_json: Value = serde_json::from_str(r2_meta.as_deref().unwrap_or("{}")).unwrap();
    assert_eq!(
        r2_meta_json["challenge"]["source"], "extracted",
        "R2 provenance must record source=extracted"
    );
    assert!(
        r2_meta_json["challenge"]["quote"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "R2 provenance must carry a non-empty source quote"
    );

    let (r4_meta,): (Option<String>,) = sqlx::query_as(
        "SELECT extraction_metadata FROM responses \
         WHERE debate_id = ? AND bot_id = ? AND round_number = 4",
    )
    .bind(&debate_id)
    .bind(&text_only_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let r4_meta_json: Value = serde_json::from_str(r4_meta.as_deref().unwrap_or("{}")).unwrap();
    assert_eq!(
        r4_meta_json["position_change"]["source"], "extracted",
        "R4 provenance must record source=extracted"
    );
    assert!(
        r4_meta_json["position_change"]["quote"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false),
        "R4 provenance must carry a non-empty source quote"
    );
}
