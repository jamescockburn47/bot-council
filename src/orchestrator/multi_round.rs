use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use tokio::sync::broadcast;
use crate::api::events::{DebateEvent, round_name};
use crate::bot_client::RoundContext;
use crate::config::{ModelsConfig, DebateConfig};
use crate::db::{models::BotRow, queries, queries_phase1};
use crate::analyser::divergence::analyse_divergence;
use crate::synthesiser::{self, precompute};
use crate::orchestrator::{rounds, state_machine};
use crate::types::{DebateId, Role};

/// Emit an SSE event. Silently drops if no sender or no listeners.
fn emit(tx: &Option<broadcast::Sender<DebateEvent>>, event: DebateEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(event); // intentional: drop if no listeners
    }
}

/// Helper to emit ResponseReceived + RoundCompleted events after a round finishes.
fn emit_round_responses(
    tx: &Option<broadcast::Sender<DebateEvent>>,
    round_number: i64,
    responses: &[crate::db::models::ResponseRow],
    pseudonym_map: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
) {
    for r in responses {
        let pseudo = pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default();
        let role_str = role_assignments.get(&r.bot_id)
            .map(|role| role.to_string())
            .unwrap_or_default();
        emit(tx, DebateEvent::ResponseReceived {
            round_number,
            pseudonym: pseudo,
            role: role_str,
            response: r.response_json.clone(),
            confidence: r.confidence,
            challenge: r.challenge_json.as_ref().and_then(|j| serde_json::from_str(j).ok()),
            position_change: r.position_change_json.as_ref().and_then(|j| serde_json::from_str(j).ok()),
            valid: r.valid,
            abstained: r.abstained,
        });
    }
    let valid_count = responses.iter().filter(|r| r.valid && !r.abstained).count();
    emit(tx, DebateEvent::RoundCompleted {
        round_number,
        response_count: responses.len(),
        valid_count,
    });
}

/// Run a full 5-round adversarial debate (Phase 1 protocol).
#[tracing::instrument(
    skip_all,
    fields(debate_id = %debate_id.as_str(), bot_count = bots.len())
)]
pub async fn run_multi_round_debate(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &DebateId,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    models_config: &ModelsConfig,
    debate_config: &DebateConfig,
    event_tx: Option<broadcast::Sender<DebateEvent>>,
) -> Result<(), String> {
    // Tag the current Sentry hub so any event (including panics) captured
    // inside this task carries debate_id. Per-tokio-task isolation is
    // provided by the spawn site binding a fresh Hub::new_from_top.
    sentry::configure_scope(|scope| {
        scope.set_tag("debate_id", debate_id.as_str());
    });
    let id = debate_id.as_str();
    let timeout = debate_config.default_timeout_secs;

    // Build maps from debate_bots table
    let debate_bots = queries_phase1::get_debate_bots_with_roles(pool, id)
        .await.map_err(|e| format!("db error: {e}"))?;
    let pseudonym_map: HashMap<String, String> = debate_bots.iter()
        .map(|db| (db.bot_id.clone(), db.pseudonym.clone()))
        .collect();
    let reverse_pseudonym_map: HashMap<String, String> = debate_bots.iter()
        .map(|db| (db.pseudonym.clone(), db.bot_id.clone()))
        .collect();
    let role_assignments: HashMap<String, Role> = debate_bots.iter()
        .filter_map(|db| {
            db.role.as_ref()
                .and_then(|r| Role::from_str(r))
                .map(|role| (db.bot_id.clone(), role))
        })
        .collect();

    // Resumption: find where to start
    let resume_round = state_machine::find_resume_point(pool, id).await?.unwrap_or(0);

    // Emit debate started
    emit(&event_tx, DebateEvent::DebateStarted {
        debate_id: id.to_string(),
        topic: topic.to_string(),
    });

    // === ROUND 0 — Blind Formation ===
    if resume_round <= 0 {
        queries::update_debate_status(pool, id, "round_0").await.map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 0).await?;
        emit(&event_tx, DebateEvent::RoundStarted {
            round_number: 0,
            name: round_name(0).to_string(),
        });
        let r0 = rounds::round0::run_round0(
            pool, client, id, topic, bots, bot_tokens, &role_assignments, timeout,
        ).await?;
        let active = r0.iter().filter(|(_, r)| r.is_some()).count();
        if active < debate_config.quorum {
            state_machine::fail_round(pool, id, 0).await?;
            queries::update_debate_status(pool, id, "failed").await.map_err(|e| format!("db: {e}"))?;
            let reason = format!("Round 0 quorum not met: {active} of {} required", debate_config.quorum);
            emit(&event_tx, DebateEvent::DebateFailed { reason: reason.clone() });
            return Err(reason);
        }
        state_machine::complete_round(pool, id, 0).await?;
        {
            let responses = queries::get_responses(pool, id, 0).await.map_err(|e| format!("db: {e}"))?;
            emit_round_responses(&event_tx, 0, &responses, &pseudonym_map, &role_assignments);
        }
    }

    // Build Round 0 context
    let r0_responses = queries::get_responses(pool, id, 0).await.map_err(|e| format!("db: {e}"))?;
    let round0_context: Vec<RoundContext> = r0_responses.iter()
        .filter(|r| !r.abstained)
        .map(|r| RoundContext {
            pseudonym: pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default(),
            round: 0, response: r.response_json.clone(), confidence: None,
        })
        .collect();

    // === ROUND 1 — Anonymous Distribution ===
    if resume_round <= 1 {
        queries::update_debate_status(pool, id, "round_1").await.map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 1).await?;
        emit(&event_tx, DebateEvent::RoundStarted {
            round_number: 1,
            name: round_name(1).to_string(),
        });
        rounds::round1::run_round1(
            pool, client, id, bots, bot_tokens, &role_assignments,
            &pseudonym_map, round0_context.clone(), timeout,
        ).await?;
        state_machine::complete_round(pool, id, 1).await?;
        {
            let responses = queries::get_responses(pool, id, 1).await.map_err(|e| format!("db: {e}"))?;
            emit_round_responses(&event_tx, 1, &responses, &pseudonym_map, &role_assignments);
        }
    }

    // Build Round 1 context
    let r1_responses = queries::get_responses(pool, id, 1).await.map_err(|e| format!("db: {e}"))?;
    let round1_context: Vec<RoundContext> = r1_responses.iter()
        .filter(|r| !r.abstained)
        .map(|r| RoundContext {
            pseudonym: pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default(),
            round: 1, response: r.response_json.clone(), confidence: r.confidence,
        })
        .collect();

    // === ROUND 2 — Structured Rebuttal ===
    if resume_round <= 2 {
        queries::update_debate_status(pool, id, "round_2").await.map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 2).await?;
        emit(&event_tx, DebateEvent::RoundStarted {
            round_number: 2,
            name: round_name(2).to_string(),
        });
        rounds::round2::run_round2(
            pool, client, id, bots, bot_tokens, &role_assignments,
            round1_context.clone(), models_config, timeout, debate_config.max_retries,
        ).await?;
        state_machine::complete_round(pool, id, 2).await?;
        {
            let responses = queries::get_responses(pool, id, 2).await.map_err(|e| format!("db: {e}"))?;
            emit_round_responses(&event_tx, 2, &responses, &pseudonym_map, &role_assignments);
        }
    }

    // Build Round 2 response map for pairing
    let r2_responses = queries::get_responses(pool, id, 2).await.map_err(|e| format!("db: {e}"))?;
    let round2_responses: HashMap<String, String> = r2_responses.iter()
        .filter(|r| !r.abstained)
        .map(|r| (pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default(), r.response_json.clone()))
        .collect();

    // === ROUND 3 — Cross-Examination ===
    if resume_round <= 3 {
        queries::update_debate_status(pool, id, "round_3").await.map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 3).await?;
        emit(&event_tx, DebateEvent::RoundStarted {
            round_number: 3,
            name: round_name(3).to_string(),
        });
        rounds::round3::run_round3(
            pool, client, id, bots, bot_tokens, &role_assignments,
            &pseudonym_map, &reverse_pseudonym_map, &round2_responses,
            models_config, timeout,
        ).await?;
        state_machine::complete_round(pool, id, 3).await?;
        {
            let responses = queries::get_responses(pool, id, 3).await.map_err(|e| format!("db: {e}"))?;
            emit_round_responses(&event_tx, 3, &responses, &pseudonym_map, &role_assignments);
        }
    }

    // Build full context for Round 4
    let all_prior = queries_phase1::get_all_responses(pool, id).await.map_err(|e| format!("db: {e}"))?;
    let full_context: Vec<RoundContext> = all_prior.iter()
        .filter(|r| !r.abstained && r.round_number <= 3)
        .map(|r| RoundContext {
            pseudonym: pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default(),
            round: r.round_number, response: r.response_json.clone(), confidence: r.confidence,
        })
        .collect();

    // === ROUND 4 — Final Position ===
    if resume_round <= 4 {
        queries::update_debate_status(pool, id, "round_4").await.map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 4).await?;
        emit(&event_tx, DebateEvent::RoundStarted {
            round_number: 4,
            name: round_name(4).to_string(),
        });
        rounds::round4::run_round4(
            pool, client, id, topic, bots, bot_tokens, &role_assignments, full_context, timeout,
        ).await?;
        state_machine::complete_round(pool, id, 4).await?;
        {
            let responses = queries::get_responses(pool, id, 4).await.map_err(|e| format!("db: {e}"))?;
            emit_round_responses(&event_tx, 4, &responses, &pseudonym_map, &role_assignments);
        }
    }

    // === DIVERGENCE ANALYSIS ===
    run_divergence_and_synthesis(
        pool, id, topic, models_config, debate_config, &pseudonym_map, &r0_responses,
        &event_tx,
    ).await
}

/// Post-Round 4: run divergence analysis per bot, then Opus synthesis.
async fn run_divergence_and_synthesis(
    pool: &SqlitePool,
    debate_id: &str,
    topic: &str,
    models_config: &ModelsConfig,
    debate_config: &DebateConfig,
    pseudonym_map: &HashMap<String, String>,
    r0_responses: &[crate::db::models::ResponseRow],
    event_tx: &Option<broadcast::Sender<DebateEvent>>,
) -> Result<(), String> {
    queries::update_debate_status(pool, debate_id, "analysing").await.map_err(|e| format!("db: {e}"))?;
    emit(event_tx, DebateEvent::SynthesisStarted);

    let r4_responses = queries::get_responses(pool, debate_id, 4).await.map_err(|e| format!("db: {e}"))?;
    let div_futures: Vec<_> = r4_responses.iter()
        .filter(|r| !r.abstained)
        .map(|r4| {
            let bot_id = r4.bot_id.clone();
            let r0_resp = r0_responses.iter()
                .find(|r| r.bot_id == bot_id && !r.abstained)
                .map(|r| r.response_json.clone())
                .unwrap_or_default();
            let r4_resp = r4.response_json.clone();
            let pc_json = r4.position_change_json.clone().unwrap_or_else(|| "{}".into());
            let config = models_config.clone();
            async move { (bot_id, analyse_divergence(&config, &r0_resp, &r4_resp, &pc_json).await) }
        })
        .collect();

    let div_results = futures::future::join_all(div_futures).await;

    for (bot_id, result) in &div_results {
        match result {
            Ok(div) => {
                let aid = uuid::Uuid::new_v4().to_string();
                let input = serde_json::json!({ "bot_id": bot_id }).to_string();
                let result_json = serde_json::to_string(div).unwrap_or_default();
                // intentional: log and continue if insert fails
                let _ = queries_phase1::insert_analysis(
                    pool, &aid, debate_id, Some(bot_id), "divergence",
                    &input, &result_json, &models_config.minimax_model,
                ).await;
            }
            Err(e) => tracing::warn!(bot_id = %bot_id, error = %e, "divergence analysis failed"),
        }
    }

    // === SYNTHESIS ===
    queries::update_debate_status(pool, debate_id, "synthesising").await.map_err(|e| format!("db: {e}"))?;

    let all_responses = queries_phase1::get_all_responses(pool, debate_id).await.map_err(|e| format!("db: {e}"))?;
    let mut transcript_lines: Vec<String> = Vec::new();
    for resp in &all_responses {
        if resp.abstained { continue; }
        let pseudo = pseudonym_map.get(&resp.bot_id).cloned().unwrap_or_default();
        transcript_lines.push(format!("[{pseudo}, Round {}]: {}", resp.round_number, resp.response_json));
    }

    let precomputed = precompute::precompute(&all_responses, pseudonym_map);
    let precomputed_json = serde_json::to_string(&precomputed).unwrap_or_default();
    let div_json: Vec<_> = div_results.iter()
        .filter_map(|(bot_id, r)| {
            r.as_ref().ok().map(|d| {
                let pseudo = pseudonym_map.get(bot_id).cloned().unwrap_or_default();
                serde_json::json!({ "pseudonym": pseudo, "analysis": d })
            })
        })
        .collect();
    let divergence_json = serde_json::to_string(&div_json).unwrap_or_default();

    let (synthesis_output, prompt_hash) = synthesiser::run_synthesis(
        models_config, topic, &transcript_lines.join("\n\n"),
        &precomputed_json, &divergence_json, debate_config.synthesis_temperature,
    ).await.map_err(|e| {
        let reason = format!("synthesis failed: {e}");
        emit(event_tx, DebateEvent::DebateFailed { reason: reason.clone() });
        reason
    })?;

    queries_phase1::insert_synthesis(pool, debate_id, &synthesis_output, &models_config.opus_model, &prompt_hash, None)
        .await.map_err(|e| format!("db error storing synthesis: {e}"))?;

    // Emit synthesis completed with parsed JSON
    let synthesis_value: serde_json::Value = serde_json::from_str(&synthesis_output)
        .unwrap_or_else(|_| serde_json::Value::String(synthesis_output));
    emit(event_tx, DebateEvent::SynthesisCompleted {
        synthesis: synthesis_value,
        citation_check: None,
    });

    queries::update_debate_status(pool, debate_id, "complete").await.map_err(|e| format!("db: {e}"))?;
    emit(event_tx, DebateEvent::DebateCompleted);
    tracing::info!(debate_id = %debate_id, "multi-round debate completed successfully");
    Ok(())
}
