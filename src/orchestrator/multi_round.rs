use crate::analyser::divergence::analyse_divergence;
use crate::api::events::{DebateEvent, round_name};
use crate::bot_client::RoundContext;
use crate::config::{DebateConfig, ModelsConfig};
use crate::db::{models::BotRow, queries, queries_phase1};
use crate::orchestrator::{rounds, state_machine};
use crate::synthesiser::{self, citation_check, precompute};
use crate::types::{DebateId, Role};
use futures::StreamExt;
use reqwest_middleware::ClientWithMiddleware;
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};
use tokio::sync::broadcast;

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
        let role_str = role_assignments
            .get(&r.bot_id)
            .map(|role| role.to_string())
            .unwrap_or_default();
        emit(
            tx,
            DebateEvent::ResponseReceived {
                round_number,
                pseudonym: pseudo,
                role: role_str,
                response: r.response_json.clone(),
                confidence: r.confidence,
                challenge: r
                    .challenge_json
                    .as_ref()
                    .and_then(|j| serde_json::from_str(j).ok()),
                position_change: r
                    .position_change_json
                    .as_ref()
                    .and_then(|j| serde_json::from_str(j).ok()),
                valid: r.valid,
                abstained: r.abstained,
            },
        );
    }
    let valid_count = responses.iter().filter(|r| r.valid && !r.abstained).count();
    emit(
        tx,
        DebateEvent::RoundCompleted {
            round_number,
            response_count: responses.len(),
            valid_count,
        },
    );
}

/// True when synthesis contains no claim-bearing sections.
fn is_conservative_empty_synthesis(s: &serde_json::Value) -> bool {
    let arrays_empty = |key: &str| {
        s.get(key)
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(false)
    };
    arrays_empty("consensus_points")
        && arrays_empty("live_disagreements")
        && arrays_empty("flagged_capitulations")
        && arrays_empty("minority_positions")
}

fn is_effective_abstention_response(text: &str) -> bool {
    let normalized = text.trim().to_lowercase();
    if normalized.is_empty() {
        return true;
    }
    let fallback_markers = [
        "(abstained)",
        "unable to formulate",
        "unable to provide",
        "cannot provide a response",
        "cannot formulate",
        "no substantive response",
        "i abstain",
    ];
    fallback_markers
        .iter()
        .any(|marker| normalized.contains(marker))
}

/// Run the configured adversarial debate protocol (5-round standard or 3-round simple mode).
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
    let id = debate_id.as_str();
    let timeout = debate_config.default_timeout_secs;
    let simple_mode = debate_config.test_mode_simple;

    // Build maps from debate_bots table
    let debate_bots = queries_phase1::get_debate_bots_with_roles(pool, id)
        .await
        .map_err(|e| format!("db error: {e}"))?;
    let pseudonym_map: HashMap<String, String> = debate_bots
        .iter()
        .map(|db| (db.bot_id.clone(), db.pseudonym.clone()))
        .collect();
    let reverse_pseudonym_map: HashMap<String, String> = debate_bots
        .iter()
        .map(|db| (db.pseudonym.clone(), db.bot_id.clone()))
        .collect();
    let role_assignments: HashMap<String, Role> = debate_bots
        .iter()
        .filter_map(|db| {
            db.role
                .as_ref()
                .and_then(|r| Role::from_str(r))
                .map(|role| (db.bot_id.clone(), role))
        })
        .collect();
    let bot_name_by_id: HashMap<String, String> = bots
        .iter()
        .map(|b| (b.id.clone(), b.name.clone()))
        .collect();
    let participant_map_text = debate_bots
        .iter()
        .map(|db| {
            let bot_name = bot_name_by_id
                .get(&db.bot_id)
                .cloned()
                .unwrap_or_else(|| db.bot_id.clone());
            format!("{} = {}", db.pseudonym, bot_name)
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Resumption: find where to start
    let resume_round = state_machine::find_resume_point(pool, id)
        .await?
        .unwrap_or(0);

    // Emit debate started
    emit(
        &event_tx,
        DebateEvent::DebateStarted {
            debate_id: id.to_string(),
            topic: topic.to_string(),
        },
    );

    // === ROUND 0 — Blind Formation ===
    if resume_round <= 0 {
        queries::update_debate_status(pool, id, "round_0")
            .await
            .map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 0).await?;
        emit(
            &event_tx,
            DebateEvent::RoundStarted {
                round_number: 0,
                name: if simple_mode {
                    "Opening"
                } else {
                    round_name(0)
                }
                .to_string(),
            },
        );
        let r0 = rounds::round0::run_round0(
            pool,
            client,
            id,
            topic,
            bots,
            bot_tokens,
            &role_assignments,
            timeout,
        )
        .await?;
        let active = r0.iter().filter(|(_, r)| r.is_some()).count();
        if active < debate_config.quorum {
            state_machine::fail_round(pool, id, 0).await?;
            queries::update_debate_status(pool, id, "failed")
                .await
                .map_err(|e| format!("db: {e}"))?;
            let reason = format!(
                "Round 0 quorum not met: {active} of {} required",
                debate_config.quorum
            );
            emit(
                &event_tx,
                DebateEvent::DebateFailed {
                    reason: reason.clone(),
                },
            );
            return Err(reason);
        }
        state_machine::complete_round(pool, id, 0).await?;
        {
            let responses = queries::get_responses(pool, id, 0)
                .await
                .map_err(|e| format!("db: {e}"))?;
            emit_round_responses(&event_tx, 0, &responses, &pseudonym_map, &role_assignments);
        }
    }

    // Build Round 0 context
    let r0_responses = queries::get_responses(pool, id, 0)
        .await
        .map_err(|e| format!("db: {e}"))?;
    let round0_context: Vec<RoundContext> = r0_responses
        .iter()
        .filter(|r| !r.abstained)
        .map(|r| RoundContext {
            pseudonym: pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default(),
            round: 0,
            response: r.response_json.clone(),
            confidence: None,
        })
        .collect();

    // === ROUND 1 — Anonymous Distribution ===
    if resume_round <= 1 {
        queries::update_debate_status(pool, id, "round_1")
            .await
            .map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 1).await?;
        emit(
            &event_tx,
            DebateEvent::RoundStarted {
                round_number: 1,
                name: if simple_mode {
                    "Rebuttal"
                } else {
                    round_name(1)
                }
                .to_string(),
            },
        );
        rounds::round1::run_round1(
            pool,
            client,
            id,
            topic,
            bots,
            bot_tokens,
            &role_assignments,
            &pseudonym_map,
            round0_context.clone(),
            timeout,
        )
        .await?;
        state_machine::complete_round(pool, id, 1).await?;
        {
            let responses = queries::get_responses(pool, id, 1)
                .await
                .map_err(|e| format!("db: {e}"))?;
            emit_round_responses(&event_tx, 1, &responses, &pseudonym_map, &role_assignments);
        }
    }

    // Build Round 1 context
    let r1_responses = queries::get_responses(pool, id, 1)
        .await
        .map_err(|e| format!("db: {e}"))?;
    let round1_context: Vec<RoundContext> = r1_responses
        .iter()
        .filter(|r| !r.abstained)
        .map(|r| RoundContext {
            pseudonym: pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default(),
            round: 1,
            response: r.response_json.clone(),
            confidence: r.confidence,
        })
        .collect();

    // === ROUND 2 — Structured Rebuttal (standard) / Final Position (simple mode) ===
    if resume_round <= 2 {
        queries::update_debate_status(pool, id, "round_2")
            .await
            .map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 2).await?;
        emit(
            &event_tx,
            DebateEvent::RoundStarted {
                round_number: 2,
                name: if simple_mode {
                    "Final Position"
                } else {
                    round_name(2)
                }
                .to_string(),
            },
        );
        rounds::round2::run_round2(
            pool,
            client,
            id,
            topic,
            bots,
            bot_tokens,
            &role_assignments,
            round1_context.clone(),
            models_config,
            timeout,
            debate_config.max_retries,
            !simple_mode,
        )
        .await?;
        state_machine::complete_round(pool, id, 2).await?;
        {
            let responses = queries::get_responses(pool, id, 2)
                .await
                .map_err(|e| format!("db: {e}"))?;
            emit_round_responses(&event_tx, 2, &responses, &pseudonym_map, &role_assignments);
        }
    }

    if simple_mode {
        return run_divergence_and_synthesis(
            pool,
            id,
            topic,
            models_config,
            debate_config,
            &pseudonym_map,
            &r0_responses,
            2,
            &participant_map_text,
            &event_tx,
        )
        .await;
    }

    // Build Round 2 response map for pairing
    let r2_responses = queries::get_responses(pool, id, 2)
        .await
        .map_err(|e| format!("db: {e}"))?;
    let round2_responses: HashMap<String, String> = r2_responses
        .iter()
        .filter(|r| !r.abstained)
        .map(|r| {
            (
                pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default(),
                r.response_json.clone(),
            )
        })
        .collect();

    // === ROUND 3 — Cross-Examination ===
    if resume_round <= 3 {
        queries::update_debate_status(pool, id, "round_3")
            .await
            .map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 3).await?;
        emit(
            &event_tx,
            DebateEvent::RoundStarted {
                round_number: 3,
                name: round_name(3).to_string(),
            },
        );
        rounds::round3::run_round3(
            pool,
            client,
            id,
            topic,
            bots,
            bot_tokens,
            &role_assignments,
            &pseudonym_map,
            &reverse_pseudonym_map,
            &round2_responses,
            models_config,
            timeout,
        )
        .await?;
        state_machine::complete_round(pool, id, 3).await?;
        {
            let responses = queries::get_responses(pool, id, 3)
                .await
                .map_err(|e| format!("db: {e}"))?;
            emit_round_responses(&event_tx, 3, &responses, &pseudonym_map, &role_assignments);
        }
    }

    // Build full context for Round 4
    let all_prior = queries_phase1::get_all_responses(pool, id)
        .await
        .map_err(|e| format!("db: {e}"))?;
    let full_context: Vec<RoundContext> = all_prior
        .iter()
        .filter(|r| !r.abstained && r.round_number <= 3)
        .map(|r| RoundContext {
            pseudonym: pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default(),
            round: r.round_number,
            response: r.response_json.clone(),
            confidence: r.confidence,
        })
        .collect();

    // === ROUND 4 — Final Position ===
    if resume_round <= 4 {
        queries::update_debate_status(pool, id, "round_4")
            .await
            .map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 4).await?;
        emit(
            &event_tx,
            DebateEvent::RoundStarted {
                round_number: 4,
                name: round_name(4).to_string(),
            },
        );
        rounds::round4::run_round4(
            pool,
            client,
            id,
            topic,
            bots,
            bot_tokens,
            &role_assignments,
            full_context,
            timeout,
        )
        .await?;
        state_machine::complete_round(pool, id, 4).await?;
        {
            let responses = queries::get_responses(pool, id, 4)
                .await
                .map_err(|e| format!("db: {e}"))?;
            emit_round_responses(&event_tx, 4, &responses, &pseudonym_map, &role_assignments);
        }
    }

    // === DIVERGENCE ANALYSIS ===
    run_divergence_and_synthesis(
        pool,
        id,
        topic,
        models_config,
        debate_config,
        &pseudonym_map,
        &r0_responses,
        4,
        &participant_map_text,
        &event_tx,
    )
    .await
}

/// Post-final-round: run divergence analysis per bot, then final synthesis.
async fn run_divergence_and_synthesis(
    pool: &SqlitePool,
    debate_id: &str,
    topic: &str,
    models_config: &ModelsConfig,
    debate_config: &DebateConfig,
    pseudonym_map: &HashMap<String, String>,
    r0_responses: &[crate::db::models::ResponseRow],
    final_round_number: i64,
    participant_map_text: &str,
    event_tx: &Option<broadcast::Sender<DebateEvent>>,
) -> Result<(), String> {
    queries::update_debate_status(pool, debate_id, "analysing")
        .await
        .map_err(|e| format!("db: {e}"))?;
    emit(event_tx, DebateEvent::SynthesisStarted);

    let final_round_responses = queries::get_responses(pool, debate_id, final_round_number)
        .await
        .map_err(|e| format!("db: {e}"))?;
    let div_futures: Vec<_> = final_round_responses
        .iter()
        .filter(|r| !r.abstained)
        .map(|final_resp| {
            let bot_id = final_resp.bot_id.clone();
            let r0_resp = r0_responses
                .iter()
                .find(|r| r.bot_id == bot_id && !r.abstained)
                .map(|r| r.response_json.clone())
                .unwrap_or_default();
            let final_text = final_resp.response_json.clone();
            let pc_json = final_resp
                .position_change_json
                .clone()
                .unwrap_or_else(|| "{}".into());
            let config = models_config.clone();
            async move {
                (
                    bot_id,
                    analyse_divergence(&config, &r0_resp, &final_text, &pc_json).await,
                )
            }
        })
        .collect();
    let div_results: Vec<_> = futures::stream::iter(div_futures)
        .buffer_unordered(models_config.analysis_max_concurrency.max(1))
        .collect()
        .await;

    for (bot_id, result) in &div_results {
        match result {
            Ok(div) => {
                let aid = uuid::Uuid::new_v4().to_string();
                let input = serde_json::json!({ "bot_id": bot_id }).to_string();
                let result_json = serde_json::to_string(div).unwrap_or_default();
                // intentional: log and continue if insert fails
                let _ = queries_phase1::insert_analysis(
                    pool,
                    &aid,
                    debate_id,
                    Some(bot_id),
                    "divergence",
                    &input,
                    &result_json,
                    models_config.effective_analysis_model(),
                )
                .await;
            }
            Err(e) => tracing::warn!(bot_id = %bot_id, error = %e, "divergence analysis failed"),
        }
    }

    // === SYNTHESIS ===
    queries::update_debate_status(pool, debate_id, "synthesising")
        .await
        .map_err(|e| format!("db: {e}"))?;

    let all_responses = queries_phase1::get_all_responses(pool, debate_id)
        .await
        .map_err(|e| format!("db: {e}"))?;
    let mut transcript_lines: Vec<String> = Vec::new();
    let mut grounding_rows: Vec<serde_json::Value> = Vec::new();
    for resp in &all_responses {
        let pseudo = pseudonym_map.get(&resp.bot_id).cloned().unwrap_or_default();
        let effective_abstained = is_effective_abstention_response(&resp.response_json);
        grounding_rows.push(serde_json::json!({
            "agent": pseudo,
            "round": resp.round_number,
            "abstained": resp.abstained,
            "effective_abstained": effective_abstained,
            "valid": resp.valid,
            "response": resp.response_json,
        }));
        if resp.abstained || effective_abstained {
            continue;
        }
        let mut lines = resp.response_json.lines();
        let first_line = lines.next().unwrap_or_default();
        let mut sanitized_response = first_line.to_string();
        for line in lines {
            sanitized_response.push('\n');
            sanitized_response.push_str("  ");
            sanitized_response.push_str(line);
        }
        transcript_lines.push(format!(
            "[{pseudo}, Round {}]: {}",
            resp.round_number, sanitized_response
        ));
    }

    let precomputed = precompute::precompute(&all_responses, pseudonym_map);
    let precomputed_json = serde_json::to_string(&precomputed).unwrap_or_default();
    let div_json: Vec<_> = div_results
        .iter()
        .filter_map(|(bot_id, r)| {
            r.as_ref().ok().map(|d| {
                let pseudo = pseudonym_map.get(bot_id).cloned().unwrap_or_default();
                serde_json::json!({ "pseudonym": pseudo, "analysis": d })
            })
        })
        .collect();
    let divergence_json = serde_json::to_string(&div_json).unwrap_or_default();
    let grounding_evidence_json = serde_json::to_string(&grounding_rows).unwrap_or_default();

    let warmup_report = synthesiser::wait_for_final_synthesis_ready(models_config).await;
    if !warmup_report.succeeded {
        tracing::warn!(
            debate_id = %debate_id,
            model = models_config.effective_final_synthesis_model(),
            attempts = warmup_report.attempts,
            elapsed_ms = warmup_report.elapsed_ms,
            "final synthesis warmup did not succeed within retry budget; continuing"
        );
    } else {
        tracing::info!(
            debate_id = %debate_id,
            model = models_config.effective_final_synthesis_model(),
            attempts = warmup_report.attempts,
            elapsed_ms = warmup_report.elapsed_ms,
            "final synthesis warmup completed"
        );
    }
    let warmup_analysis_id = uuid::Uuid::new_v4().to_string();
    let warmup_input = serde_json::json!({
        "base_url": models_config.effective_final_synthesis_base_url(),
        "model": models_config.effective_final_synthesis_model(),
    });
    let warmup_result = serde_json::json!({
        "attempts": warmup_report.attempts,
        "elapsed_ms": warmup_report.elapsed_ms,
        "succeeded": warmup_report.succeeded,
    });
    let _ = queries_phase1::insert_analysis(
        pool,
        &warmup_analysis_id,
        debate_id,
        None,
        "synthesis_warmup",
        &warmup_input.to_string(),
        &warmup_result.to_string(),
        models_config.effective_final_synthesis_model(),
    )
    .await;

    let (synthesis_output, prompt_hash) = synthesiser::run_synthesis(
        models_config,
        topic,
        participant_map_text,
        &transcript_lines.join("\n\n"),
        &precomputed_json,
        &divergence_json,
        &grounding_evidence_json,
        debate_config.synthesis_temperature,
    )
    .await
    .map_err(|e| {
        let reason = format!("synthesis failed: {e}");
        emit(
            event_tx,
            DebateEvent::DebateFailed {
                reason: reason.clone(),
            },
        );
        reason
    })?;

    let synthesis_value: serde_json::Value = serde_json::from_str(&synthesis_output)
        .map_err(|e| format!("failed to parse synthesis JSON for citation check: {e}"))?;

    let valid_pseudonyms: HashSet<String> = pseudonym_map.values().cloned().collect();
    let responses_by_pseudonym_round: HashMap<(String, i64), bool> = all_responses
        .iter()
        .filter_map(|r| {
            pseudonym_map.get(&r.bot_id).cloned().map(|pseudo| {
                (
                    (pseudo, r.round_number),
                    r.abstained || is_effective_abstention_response(&r.response_json),
                )
            })
        })
        .collect();
    let citation_result = citation_check::check_citations(
        &synthesis_value,
        &valid_pseudonyms,
        &responses_by_pseudonym_round,
        final_round_number,
    );
    if !citation_result.citations_invalid.is_empty() {
        tracing::warn!(
            debate_id = %debate_id,
            total = citation_result.citations_total,
            invalid = citation_result.citations_invalid.len(),
            "synthesis contains invalid citations; accepting with warning"
        );
    }
    if citation_result.citations_total == 0 && !is_conservative_empty_synthesis(&synthesis_value) {
        tracing::warn!(
            debate_id = %debate_id,
            "synthesis contains substantive content without citations; accepting with warning"
        );
    }
    let citation_json = serde_json::to_string(&citation_result)
        .map_err(|e| format!("failed to serialize citation check: {e}"))?;

    queries_phase1::insert_synthesis(
        pool,
        debate_id,
        &synthesis_output,
        models_config.effective_final_synthesis_model(),
        &prompt_hash,
        Some(&citation_json),
    )
    .await
    .map_err(|e| format!("db error storing synthesis: {e}"))?;

    // Emit synthesis completed with parsed JSON
    emit(
        event_tx,
        DebateEvent::SynthesisCompleted {
            synthesis: synthesis_value,
            citation_check: serde_json::from_str(&citation_json).ok(),
        },
    );

    queries::update_debate_status(pool, debate_id, "complete")
        .await
        .map_err(|e| format!("db: {e}"))?;
    emit(event_tx, DebateEvent::DebateCompleted);
    tracing::info!(debate_id = %debate_id, "multi-round debate completed successfully");
    Ok(())
}
