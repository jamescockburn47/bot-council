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

/// True when a bot's stored response is a fallback/non-answer string —
/// "(abstained)", "unable to formulate", etc. — even though the
/// `abstained` column on its row is false. These show up when a bot
/// timed out or returned a stock polite refusal; treating them as
/// substantive pollutes the synthesis input.
///
/// Exposed so the resynth CLI uses the same markers as the live
/// orchestrator (drift between the two produced empty-synthesis
/// regressions during the MiniMax rerun).
pub fn is_effective_abstention_response(text: &str) -> bool {
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

/// Run the 5-round adversarial debate protocol.
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
                name: round_name(0).to_string(),
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
                name: round_name(1).to_string(),
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

    // === ROUND 2 — Structured Rebuttal ===
    if resume_round <= 2 {
        queries::update_debate_status(pool, id, "round_2")
            .await
            .map_err(|e| format!("db: {e}"))?;
        state_machine::start_round(pool, id, 2).await?;
        emit(
            &event_tx,
            DebateEvent::RoundStarted {
                round_number: 2,
                name: round_name(2).to_string(),
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

    // === CRUX SELECTION (between R2 and R3) ===
    // Pick the single most-divergent R1 claim. If MiniMax returns a
    // valid + substring-verified selection, R3 runs the crux-engagement
    // prompt; otherwise we fall back to legacy cross-examination so R3
    // still produces adversarial engagement.
    let r1_entries: Vec<crate::analyser::crux::R1Entry> = r1_responses
        .iter()
        .filter(|r| !r.abstained)
        .filter_map(|r| {
            let pseudonym = pseudonym_map.get(&r.bot_id).cloned()?;
            let r0_text = r0_responses
                .iter()
                .find(|r0| r0.bot_id == r.bot_id && !r0.abstained)
                .map(|r0| r0.response_json.clone())
                .unwrap_or_default();
            Some(crate::analyser::crux::R1Entry {
                pseudonym,
                r0: r0_text,
                r1: r.response_json.clone(),
            })
        })
        .collect();
    let crux_result = crate::analyser::crux::select_crux(models_config, topic, &r1_entries).await;
    if let Ok(ref c) = crux_result {
        let aid = uuid::Uuid::new_v4().to_string();
        let input = serde_json::to_string(&r1_entries).unwrap_or_default();
        let result = serde_json::to_string(c).unwrap_or_default();
        // intentional: log and continue if insert fails — R3 should
        // still dispatch even if we can't persist the analysis row.
        let _ = queries_phase1::insert_analysis(
            pool,
            &aid,
            id,
            None,
            "crux_selection",
            &input,
            &result,
            models_config.effective_analysis_model(),
        )
        .await;
    } else if let Err(e) = &crux_result {
        tracing::warn!(
            error = ?e,
            "crux selection failed; R3 will use cross-examination fallback"
        );
    }

    // === ROUND 3 — Crux Engagement (legacy cross-exam if selection failed) ===
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
            crux_result.as_ref().ok(),
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

    // Preserve R3 responses by bot_id so the divergence analyser can
    // classify each bot's crux_shift (R1→R3 movement on the selected
    // crux claim). Empty if R3 was skipped / fully abstained.
    let r3_text_by_bot: HashMap<String, String> = all_prior
        .iter()
        .filter(|r| r.round_number == 3 && !r.abstained)
        .map(|r| (r.bot_id.clone(), r.response_json.clone()))
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
            models_config,
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
    // Pass the selected crux (if crux selection succeeded) and each bot's
    // R3 text so `analyse_divergence` can populate `crux_shift` per-bot.
    // None of the three inputs (r0/r3/crux) are required; when any is
    // missing the divergence record simply omits `crux_shift`.
    let crux_claim_owned = crux_result.as_ref().ok().map(|c| c.claim.clone());
    run_divergence_and_synthesis(
        pool,
        id,
        topic,
        models_config,
        debate_config,
        bots,
        bot_tokens,
        &pseudonym_map,
        &r0_responses,
        4,
        &participant_map_text,
        crux_claim_owned.as_deref(),
        crux_result.as_ref().ok(),
        &r3_text_by_bot,
        &event_tx,
    )
    .await
}

/// Post-final-round: run divergence analysis per bot, run peer scoring
/// across participating bots, then run final synthesis.
///
/// `crux_claim` is `Some` only when crux selection between R2 and R3
/// succeeded; `r3_text_by_bot` is empty if R3 fully abstained. Both feed
/// the per-bot `crux_shift` classification in `analyse_divergence`.
/// `crux` (the full `CruxSelection`) is also threaded into the synthesis
/// prompt so the synthesiser can emit a `crux_outcome` summary.
#[allow(clippy::too_many_arguments)]
async fn run_divergence_and_synthesis(
    pool: &SqlitePool,
    debate_id: &str,
    topic: &str,
    models_config: &ModelsConfig,
    debate_config: &DebateConfig,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    pseudonym_map: &HashMap<String, String>,
    r0_responses: &[crate::db::models::ResponseRow],
    final_round_number: i64,
    participant_map_text: &str,
    crux_claim: Option<&str>,
    crux: Option<&crate::analyser::crux::CruxSelection>,
    r3_text_by_bot: &HashMap<String, String>,
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
            let r3_text = r3_text_by_bot.get(&bot_id).cloned();
            let crux_claim_owned = crux_claim.map(|s| s.to_string());
            let config = models_config.clone();
            async move {
                (
                    bot_id,
                    analyse_divergence(
                        &config,
                        &r0_resp,
                        &final_text,
                        &pc_json,
                        crux_claim_owned.as_deref(),
                        r3_text.as_deref(),
                    )
                    .await,
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

    // === PEER SCORING ===
    // Ask each bot to rate the others' final-round positions. Failures
    // are logged and skipped so one unresponsive bot does not block the
    // debate from reaching synthesis. Feature absence was the reason
    // `results.rankings` always showed total_scores=0 on Phase 1 debates.
    if let Err(e) = run_peer_scoring(
        pool,
        debate_id,
        bots,
        bot_tokens,
        pseudonym_map,
        &final_round_responses,
        final_round_number,
    )
    .await
    {
        tracing::warn!(
            debate_id,
            error = %e,
            "peer scoring failed; continuing to synthesis without rankings"
        );
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
        crux,
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

/// Dispatch the peer-scoring round: ask each bot to rate the others'
/// final-round positions, then persist the returned scores into
/// `peer_scores`. `results.rankings` then aggregates over those rows via
/// `queries::get_peer_scores`.
///
/// Per-bot failures (timeout, bad JSON, HTTP error, abstention) are
/// logged at warn and skipped — one unresponsive bot never blocks the
/// rest from contributing their scores.
///
/// Mirrors the Phase 0 scoring pattern in `src/orchestrator/mod.rs` so a
/// single source of truth for the wire contract stays with `bot_client`.
#[allow(clippy::too_many_arguments)]
async fn run_peer_scoring(
    pool: &SqlitePool,
    debate_id: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    pseudonym_map: &HashMap<String, String>,
    final_round_responses: &[crate::db::models::ResponseRow],
    final_round_number: i64,
) -> Result<(), String> {
    use crate::bot_client::{self, ScoringContext, ScoringRequest};
    use reqwest_middleware::ClientBuilder as MwClientBuilder;
    use std::time::Duration;

    // Build anonymised contexts from final-round responses only (the
    // bots' committed final positions are the right substance to score).
    let mut anonymised: Vec<ScoringContext> = Vec::new();
    for resp in final_round_responses {
        if resp.abstained || !resp.valid {
            continue;
        }
        if is_effective_abstention_response(&resp.response_json) {
            continue;
        }
        let pseudonym = match pseudonym_map.get(&resp.bot_id) {
            Some(p) => p.clone(),
            None => continue,
        };
        anonymised.push(ScoringContext {
            pseudonym,
            response: resp.response_json.clone(),
        });
    }
    if anonymised.len() < 2 {
        tracing::info!(
            debate_id,
            substantive_finalists = anonymised.len(),
            "peer scoring skipped: fewer than 2 substantive finalists"
        );
        return Ok(());
    }

    // Dedicated HTTP client for the scoring round — 5min per-bot budget
    // matches the per-round timeout elsewhere.
    let base_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("build scoring client: {e}"))?;
    let client = MwClientBuilder::new(base_client).build();

    let scoring_futures: Vec<_> = bots
        .iter()
        .map(|bot| {
            let client = client.clone();
            let endpoint = bot.endpoint_url.clone();
            let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
            let session_id = debate_id.to_string();
            let own_pseudonym = pseudonym_map.get(&bot.id).cloned().unwrap_or_default();
            let context: Vec<ScoringContext> = anonymised
                .iter()
                .filter(|c| c.pseudonym != own_pseudonym)
                .cloned()
                .collect();
            let bot_id = bot.id.clone();
            async move {
                if context.is_empty() {
                    return (bot_id, None);
                }
                let req = ScoringRequest {
                    session_id,
                    round: "scoring".to_string(),
                    context,
                    prompt: format!(
                        "You are rating the OTHER participants' round-{final_round_number} positions shown in `context`. For each one, return a JSON object under `scores` with fields {{\"pseudonym\": string, \"reasoning_quality\": 0-10 int, \"factual_grounding\": 0-10 int, \"overall\": 0-10 int, \"reasoning\": short string}}. Do not score yourself. Return exactly {{\"scores\": [...]}} — no prose outside that JSON object."
                    ),
                };
                match tokio::time::timeout(
                    Duration::from_secs(300),
                    bot_client::send_scoring_request(&client, &endpoint, &token, &req),
                )
                .await
                {
                    Ok(Ok(resp)) => (bot_id, Some(resp.scores)),
                    Ok(Err(e)) => {
                        tracing::warn!(bot_id = %bot_id, error = %e, "peer scoring request failed");
                        (bot_id, None)
                    }
                    Err(_) => {
                        tracing::warn!(bot_id = %bot_id, "peer scoring request timed out");
                        (bot_id, None)
                    }
                }
            }
        })
        .collect();

    let scoring_results = futures::future::join_all(scoring_futures).await;

    let mut inserted = 0usize;
    for (scorer_bot_id, scores_opt) in &scoring_results {
        let Some(scores) = scores_opt else { continue };
        for score in scores {
            // Clamp scores to the 0-10 range so an out-of-bounds bot
            // response can't pollute aggregation downstream.
            let rq = score.reasoning_quality.clamp(0, 10);
            let fg = score.factual_grounding.clamp(0, 10);
            let ov = score.overall.clamp(0, 10);
            let score_id = uuid::Uuid::new_v4().to_string();
            match crate::db::queries::insert_peer_score(
                pool,
                &score_id,
                debate_id,
                scorer_bot_id,
                &score.pseudonym,
                rq,
                fg,
                ov,
                &score.reasoning,
            )
            .await
            {
                Ok(_) => inserted += 1,
                Err(e) => tracing::warn!(
                    scorer = %scorer_bot_id,
                    target = %score.pseudonym,
                    error = %e,
                    "peer_scores insert failed"
                ),
            }
        }
    }

    tracing::info!(
        debate_id,
        scorers = scoring_results.len(),
        scores_inserted = inserted,
        "peer scoring completed"
    );
    Ok(())
}
