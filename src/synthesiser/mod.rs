/// LLM-backed effective-abstention classifier (synthesis prep).
pub mod abstention_classifier;
/// Post-synthesis citation validation.
pub mod citation_check;
/// OpenAI-compatible chat client for the final-synthesis model.
mod client;
/// Transcript / grounding-evidence parsing shared across the module.
mod evidence;
/// Pre-computation of structural debate data.
pub mod precompute;
/// Synthesis prompt construction.
mod prompt;
/// Output schema for the synthesis result.
pub mod schema;

use crate::analyser::crux::CruxSelection;
use crate::config::ModelsConfig;
use crate::synthesiser::schema::SynthesisOutput;
use client::{
    call_local_synthesis_model, call_model_json, LocalChatCompletionRequest, LocalChatMessage,
    LocalResponseFormat,
};
use evidence::{
    parse_grounding_evidence, parse_participant_map, parse_transcript_entries, summarize_for_meta,
    would_exceed_budget, GroundingEvidenceEntry,
};
use prompt::build_synthesis_prompt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub struct FinalSynthesisWarmupReport {
    pub attempts: u32,
    pub elapsed_ms: u128,
    /// False when warmup was enabled but did not succeed within retry budget.
    pub succeeded: bool,
}

/// Produce the final synthesis with EVO's local model.
///
/// Returns `(synthesis_json_string, prompt_hash)` on success, or an error
/// message on failure.
///
/// # Arguments
/// * `config` — model configuration (keys, model names, base URLs)
/// * `topic` — the debate topic
/// * `transcript_text` — full anonymised transcript
/// * `precomputed_json` — serialised [`precompute::PrecomputedData`]
/// * `divergence_results_json` — serialised divergence analyses
/// * `crux` — `Some` when crux selection succeeded between R2/R3; drives an
///   extra "Crux outcome" section in the synthesis prompt.
/// * `temperature` — requested sampling temperature (clamped to low range)
#[allow(clippy::too_many_arguments)]
pub async fn run_synthesis(
    config: &ModelsConfig,
    topic: &str,
    participant_map_text: &str,
    transcript_text: &str,
    precomputed_json: &str,
    divergence_results_json: &str,
    grounding_evidence_json: &str,
    crux: Option<&CruxSelection>,
    temperature: f64,
) -> Result<(String, String), String> {
    let system_prompt = build_synthesis_prompt(
        topic,
        participant_map_text,
        transcript_text,
        precomputed_json,
        divergence_results_json,
        grounding_evidence_json,
        crux,
    );

    let prompt_hash = {
        let mut hasher = Sha256::new();
        hasher.update(system_prompt.as_bytes());
        hex::encode(hasher.finalize())
    };

    // Retry-on-empty: MiniMax-M2.7 occasionally returns a shaped JSON with
    // all-empty structural arrays (consensus/disagreements/minorities),
    // which triggers our salvage path and writes a vapid stub. Empirically
    // an immediate retry — often with slightly bumped temperature —
    // produces correct content. We attempt up to 3 times before accepting
    // the empty result (which itself still has the structural-salvage
    // safety net downstream).
    let transcript_has_substance = transcript_text.trim().len() > 500;
    let max_attempts: u32 = 3;
    let mut parsed: Option<SynthesisOutput> = None;
    for attempt in 1..=max_attempts {
        let attempt_temp = (temperature + 0.1 * f64::from(attempt - 1)).min(0.5);
        let content = match call_local_synthesis_model(config, &system_prompt, attempt_temp).await {
            Ok(c) => c,
            Err(e) => {
                if attempt < max_attempts {
                    tracing::warn!(error = %e, attempt, "local synthesis model call failed; retrying");
                    continue;
                }
                tracing::warn!(error = %e, attempts = attempt, "local synthesis model failed after retries; using conservative fallback");
                let mut fallback = conservative_fallback(topic, precomputed_json);
                ensure_substantive_meta(
                    &mut fallback,
                    participant_map_text,
                    transcript_text,
                    grounding_evidence_json,
                );
                let canonical = serde_json::to_string(&fallback)
                    .map_err(|se| format!("failed to serialise fallback synthesis output: {se}"))?;
                return Ok((canonical, prompt_hash));
            }
        };

        // Parse as a loose Value first and reinject the known `topic` if the
        // model omitted or nulled it — MiniMax-M2.7 sometimes drops it.
        let attempt_parsed = match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(mut value) => {
                if let Some(obj) = value.as_object_mut() {
                    let topic_missing = match obj.get("topic") {
                        None => true,
                        Some(serde_json::Value::Null) => true,
                        Some(serde_json::Value::String(s)) if s.trim().is_empty() => true,
                        _ => false,
                    };
                    if topic_missing {
                        obj.insert("topic".into(), serde_json::Value::String(topic.to_string()));
                    }
                }
                match serde_json::from_value::<SynthesisOutput>(value.clone()) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(error = %e, attempt, "non-schema JSON after topic-reinject; salvaging");
                        salvage_loose_output(
                            topic,
                            participant_map_text,
                            transcript_text,
                            precomputed_json,
                            grounding_evidence_json,
                            &value,
                        )
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, attempt, "unparseable JSON; using fallback");
                conservative_fallback(topic, precomputed_json)
            }
        };

        let is_structurally_empty = attempt_parsed.consensus_points.is_empty()
            && attempt_parsed.live_disagreements.is_empty()
            && attempt_parsed.minority_positions.is_empty();
        if is_structurally_empty && transcript_has_substance && attempt < max_attempts {
            tracing::warn!(
                attempt,
                next_temperature = (temperature + 0.1 * f64::from(attempt)).min(0.5),
                "synthesis returned all-empty structural arrays despite substantive transcript; retrying"
            );
            continue;
        }

        parsed = Some(attempt_parsed);
        break;
    }
    let mut parsed =
        parsed.expect("retry loop must either return early or produce a SynthesisOutput");
    // Hardening: if the model returned all-empty structured arrays but the
    // transcript contains substantive non-abstained content, derive one
    // minority_position per participating bot so the downstream argument
    // map still has nodes. Prompt changes already push the model toward
    // populated output; this is the last-resort safety net.
    enrich_empty_output_with_structural_minorities(
        &mut parsed,
        transcript_text,
        grounding_evidence_json,
    );
    ensure_substantive_meta(
        &mut parsed,
        participant_map_text,
        transcript_text,
        grounding_evidence_json,
    );
    let canonical = serde_json::to_string(&parsed)
        .map_err(|e| format!("failed to serialise synthesis output: {e}"))?;

    Ok((canonical, prompt_hash))
}

/// Ensure the final synthesis model is warm and accepting requests.
///
/// Never fails the caller: on retry exhaustion returns
/// [`FinalSynthesisWarmupReport::succeeded`] = false so debates can still finish
/// (synthesis may use schema fallback). Use `final_synthesis_warmup_max_attempts
/// = 0` for infinite retries (block-until-ready) when that behaviour is intended.
pub async fn wait_for_final_synthesis_ready(config: &ModelsConfig) -> FinalSynthesisWarmupReport {
    if !config.final_synthesis_warmup_enabled {
        return FinalSynthesisWarmupReport {
            attempts: 0,
            elapsed_ms: 0,
            succeeded: true,
        };
    }
    let started = std::time::Instant::now();
    let request = LocalChatCompletionRequest {
        model: config.effective_final_synthesis_model().to_string(),
        temperature: 0.0,
        top_k: 1,
        seed: 42,
        cache_prompt: false,
        reasoning_format: "none".into(),
        response_format: LocalResponseFormat {
            format_type: "json_object".into(),
            schema: None,
        },
        messages: vec![LocalChatMessage {
            role: "user".into(),
            content: "Return exactly this JSON object: {\"ready\":true}".into(),
        }],
    };

    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match call_model_json(config, &request, true).await {
            Ok(content) => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if value.get("ready").and_then(|v| v.as_bool()) == Some(true) {
                        tracing::info!(
                            attempt,
                            model = config.effective_final_synthesis_model(),
                            "final synthesis model warmup successful"
                        );
                        return FinalSynthesisWarmupReport {
                            attempts: attempt,
                            elapsed_ms: started.elapsed().as_millis(),
                            succeeded: true,
                        };
                    }
                }
                tracing::warn!(
                    attempt,
                    response = %content,
                    "final synthesis warmup returned unexpected payload"
                );
            }
            Err(err) => {
                tracing::warn!(attempt, error = %err, "final synthesis warmup probe failed");
            }
        }

        if config.final_synthesis_warmup_max_attempts > 0
            && attempt >= config.final_synthesis_warmup_max_attempts
        {
            tracing::warn!(
                attempt,
                max_attempts = config.final_synthesis_warmup_max_attempts,
                model = config.effective_final_synthesis_model(),
                base_url = config.effective_final_synthesis_base_url(),
                "final synthesis warmup exhausted; continuing without verified warmup"
            );
            return FinalSynthesisWarmupReport {
                attempts: attempt,
                elapsed_ms: started.elapsed().as_millis(),
                succeeded: false,
            };
        }
        tokio::time::sleep(Duration::from_secs(
            config.final_synthesis_warmup_delay_secs,
        ))
        .await;
    }
}


/// Last-resort hardening: when the model returns all-empty structured
/// arrays but the transcript has substantive non-abstained content,
/// synthesise one minority_position per participating bot from their
/// latest substantive round. Ensures the argument map has nodes even
/// when the model fails to extract structure.
///
/// No-op if any structured array is already populated — we never
/// overwrite model output, only enrich empties.
fn enrich_empty_output_with_structural_minorities(
    output: &mut SynthesisOutput,
    transcript_text: &str,
    grounding_evidence_json: &str,
) {
    // Fire only when the model produced NOTHING structured. If any list
    // has content, trust the model's judgment about what to emit.
    let all_empty = output.consensus_points.is_empty()
        && output.live_disagreements.is_empty()
        && output.minority_positions.is_empty()
        && output.flagged_capitulations.is_empty();
    if !all_empty {
        return;
    }

    // Source truth for substantive responses: grounding-evidence if
    // available (more reliable — has abstained/valid flags), otherwise
    // fall back to parsed transcript lines.
    let mut evidence = parse_grounding_evidence(grounding_evidence_json);
    if evidence.is_empty() {
        evidence = parse_transcript_entries(transcript_text)
            .into_iter()
            .map(|entry| GroundingEvidenceEntry {
                agent: entry.agent,
                round: entry.round,
                abstained: false,
                effective_abstained: false,
                valid: true,
                response: entry.text,
            })
            .collect();
    }
    if evidence.is_empty() {
        return;
    }

    // For each agent, find the latest round with a substantive response
    // (not abstained, valid, and non-trivially long).
    let mut latest_by_agent: HashMap<String, GroundingEvidenceEntry> = HashMap::new();
    for entry in evidence {
        if entry.abstained || entry.effective_abstained || !entry.valid {
            continue;
        }
        if entry.response.trim().len() < 40 {
            continue; // stub / marker response; skip
        }
        match latest_by_agent.get(&entry.agent) {
            Some(existing) if existing.round >= entry.round => {}
            _ => {
                latest_by_agent.insert(entry.agent.clone(), entry);
            }
        }
    }

    if latest_by_agent.is_empty() {
        return;
    }

    // Stable ordering — sort by pseudonym so the graph layout is
    // deterministic across reruns.
    let mut agents: Vec<String> = latest_by_agent.keys().cloned().collect();
    agents.sort();

    for agent in agents {
        let entry = &latest_by_agent[&agent];
        let snippet = summarize_for_meta(&entry.response, 280);
        let headline = format!("{} position (R{})", agent, entry.round);
        output.minority_positions.push(crate::synthesiser::schema::MinorityPosition {
            bot: agent.clone(),
            headline,
            position: snippet,
            key_argument: format!("Derived from [{}, Round {}] — no structured argument extracted by the synthesiser.", agent, entry.round),
            confidence: None,
        });
    }

    tracing::info!(
        minorities_added = output.minority_positions.len(),
        "synthesis: empty output enriched with structural minority positions"
    );
}

/// Build a deterministic no-hallucination fallback from structural data only.
fn conservative_fallback(topic: &str, precomputed_json: &str) -> SynthesisOutput {
    #[derive(Debug, Deserialize)]
    struct PrecomputedFallback {
        #[serde(default)]
        confidence_trajectories: HashMap<String, Vec<Option<i64>>>,
    }

    let confidence_trajectories = serde_json::from_str::<PrecomputedFallback>(precomputed_json)
        .map(|p| p.confidence_trajectories)
        .unwrap_or_default();

    SynthesisOutput {
        topic: topic.to_string(),
        executive_summary: String::new(),
        consensus_points: Vec::new(),
        live_disagreements: Vec::new(),
        flagged_capitulations: Vec::new(),
        minority_positions: Vec::new(),
        confidence_trajectories,
        meta_observations: "Conservative fallback synthesis: only structured confidence trajectories are reported because the local model output could not be validated against the required schema.".into(),
    }
}

/// Convert non-conforming model JSON into safe synthesis output.
fn salvage_loose_output(
    topic: &str,
    participant_map_text: &str,
    transcript_text: &str,
    precomputed_json: &str,
    grounding_evidence_json: &str,
    loose: &serde_json::Value,
) -> SynthesisOutput {
    let mut output = conservative_fallback(topic, precomputed_json);
    if let Some(loose_topic) = loose.get("topic").and_then(|v| v.as_str()) {
        if !loose_topic.trim().is_empty() {
            output.topic = loose_topic.to_string();
        }
    }
    if let Some(summary) = loose.get("executive_summary").and_then(|v| v.as_str()) {
        if !summary.trim().is_empty() {
            output.executive_summary = summary.trim().to_string();
        }
    }
    let meta = loose
        .get("meta_observations")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            let consensus = loose
                .get("consensus_points")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            let disagreements = loose
                .get("live_disagreements")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            let combined = [consensus, disagreements]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" | ");
            if combined.is_empty() {
                None
            } else {
                Some(combined)
            }
        });
    if let Some(meta) = meta {
        output.meta_observations = meta;
    }
    ensure_substantive_meta(
        &mut output,
        participant_map_text,
        transcript_text,
        grounding_evidence_json,
    );
    output
}


/// Ensure meta observations are grounded in deterministic transcript evidence.
fn ensure_substantive_meta(
    output: &mut SynthesisOutput,
    participant_map_text: &str,
    transcript_text: &str,
    grounding_evidence_json: &str,
) {
    output.meta_observations = compose_structured_meta(
        output,
        participant_map_text,
        transcript_text,
        grounding_evidence_json,
    );
    if !output
        .meta_observations
        .trim_start()
        .starts_with("Conclusion:")
    {
        output.meta_observations = format!("Conclusion: {}", output.meta_observations.trim());
    }
}

fn compose_structured_meta(
    output: &SynthesisOutput,
    participant_map_text: &str,
    transcript_text: &str,
    grounding_evidence_json: &str,
) -> String {
    let mut sections = Vec::new();
    sections.push(format!("Conclusion: {}", derive_overall_outcome(output)));

    let summary_of_arguments = if output.live_disagreements.is_empty() {
        if output.consensus_points.is_empty() {
            // No consensus AND no disagreements — but minorities may still
            // be present. Surface them rather than claiming nothing was
            // extracted.
            if output.minority_positions.is_empty() {
                "- Structured argument map not extracted; see the raw transcript for per-bot positions.".to_string()
            } else {
                output
                    .minority_positions
                    .iter()
                    .take(4)
                    .map(|m| format!("- {} held: {}", m.bot, summarize_for_meta(&m.position, 200)))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        } else {
            let top = output
                .consensus_points
                .iter()
                .take(3)
                .map(|c| format!("- {}", summarize_for_meta(&c.point, 200)))
                .collect::<Vec<_>>()
                .join("\n");
            if top.is_empty() {
                "- Structured argument map not extracted; see the raw transcript for per-bot positions."
                    .to_string()
            } else {
                top
            }
        }
    } else {
        output
            .live_disagreements
            .iter()
            .take(4)
            .map(|d| {
                format!(
                    "- {}: {} ({}) vs {} ({})",
                    summarize_for_meta(&d.issue, 120),
                    summarize_for_meta(&d.side_a.position, 120),
                    d.side_a.bots.join(", "),
                    summarize_for_meta(&d.side_b.position, 120),
                    d.side_b.bots.join(", ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections.push(format!("Summary of arguments:\n{summary_of_arguments}"));

    let disagreements = if output.live_disagreements.is_empty() {
        "- No live disagreement remained at synthesis time.".to_string()
    } else {
        output
            .live_disagreements
            .iter()
            .take(4)
            .map(|d| {
                format!(
                    "- {} | A: {} | B: {}",
                    summarize_for_meta(&d.issue, 90),
                    summarize_for_meta(&d.side_a.best_argument, 180),
                    summarize_for_meta(&d.side_b.best_argument, 180)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections.push(format!("Key disagreements:\n{disagreements}"));

    let minority_positions = if output.minority_positions.is_empty() {
        "- No explicit minority position was preserved in this run.".to_string()
    } else {
        output
            .minority_positions
            .iter()
            .take(4)
            .map(|m| {
                format!(
                    "- {}: {} | {}",
                    m.bot,
                    summarize_for_meta(&m.position, 120),
                    summarize_for_meta(&m.key_argument, 180)
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections.push(format!("Minority positions:\n{minority_positions}"));

    let outcome = format!(
        "- Consensus points: {}\n- Live disagreements: {}\n- Flagged capitulations: {}",
        output.consensus_points.len(),
        output.live_disagreements.len(),
        output.flagged_capitulations.len()
    );
    sections.push(format!("Overall outcome:\n{outcome}"));

    let behavior = build_behavior_notes(
        participant_map_text,
        transcript_text,
        grounding_evidence_json,
    );
    sections.push(format!("Bot behaviour notes:\n{behavior}"));

    sections.join("\n\n")
}

fn derive_overall_outcome(output: &SynthesisOutput) -> String {
    let has_consensus = !output.consensus_points.is_empty();
    let has_disagreements = !output.live_disagreements.is_empty();
    let has_minorities = !output.minority_positions.is_empty();

    match (has_consensus, has_disagreements, has_minorities) {
        (true, true, _) => {
            "Partial convergence: some points aligned, but core disputes remained unresolved."
                .to_string()
        }
        (true, false, _) => {
            "Broad alignment: no unresolved core disagreement remained in the final synthesis."
                .to_string()
        }
        (false, true, _) => {
            "No consensus: arguments remained materially contested at close.".to_string()
        }
        (false, false, true) => {
            "No shared structure emerged: each participating bot held a distinct position; see minority positions.".to_string()
        }
        (false, false, false) => {
            "No structured argument map was extracted; the full per-bot positions remain in the transcript.".to_string()
        }
    }
}

/// Build a deterministic, evidence-grounded position walkthrough.
fn derive_position_narrative(
    participant_map_text: &str,
    transcript_text: &str,
    grounding_evidence_json: &str,
) -> String {
    const META_MAX_CHARS: usize = 3600;
    let mut evidence = parse_grounding_evidence(grounding_evidence_json);
    if evidence.is_empty() {
        evidence = parse_transcript_entries(transcript_text)
            .into_iter()
            .map(|entry| GroundingEvidenceEntry {
                agent: entry.agent,
                round: entry.round,
                abstained: false,
                effective_abstained: false,
                valid: true,
                response: entry.text,
            })
            .collect();
    }
    if evidence.is_empty() {
        return "Conclusion: No transcript evidence was available for a grounded position walkthrough.".into();
    }

    let alias_map = parse_participant_map(participant_map_text);

    let mut agents: Vec<String> = evidence.iter().map(|e| e.agent.clone()).collect();
    agents.sort();
    agents.dedup();
    let max_round = evidence.iter().map(|e| e.round).max().unwrap_or(0);

    let mut sections = Vec::new();
    let intro = format!(
        "Evidence-grounded walkthrough built from {} recorded round events across rounds 0-{max_round}.",
        evidence.len()
    );
    let mut char_budget = intro.len();
    sections.push(intro);

    for agent in agents {
        let mut agent_entries: Vec<&GroundingEvidenceEntry> =
            evidence.iter().filter(|e| e.agent == agent).collect();
        agent_entries.sort_by_key(|e| e.round);
        let non_abstained: Vec<&GroundingEvidenceEntry> = agent_entries
            .iter()
            .copied()
            .filter(|e| !e.abstained && !e.effective_abstained)
            .collect();

        let round_ledger = (0..=max_round)
            .map(|r| match agent_entries.iter().find(|e| e.round == r) {
                Some(entry) if entry.effective_abstained => format!("R{r}=effective-abstention"),
                Some(entry) if entry.abstained => format!("R{r}=abstained"),
                Some(entry) if !entry.valid => format!("R{r}=invalid"),
                Some(_) => format!("R{r}=responded"),
                None => format!("R{r}=no-record"),
            })
            .collect::<Vec<_>>()
            .join(", ");

        let label = alias_map
            .get(&agent)
            .map(|name| format!("{agent} ({name})"))
            .unwrap_or_else(|| agent.clone());
        let Some(opening) = non_abstained.first().copied() else {
            sections.push(format!(
                "{label}: no non-abstained response content is available. Round ledger: {round_ledger}."
            ));
            continue;
        };
        let latest = non_abstained.last().copied().unwrap_or(opening);
        let opening_summary = summarize_for_meta(&opening.response, 220);
        let latest_summary = summarize_for_meta(&latest.response, 260);
        let section = format!(
            "{label}: round ledger -> {round_ledger}. Opening claim [{agent}, Round {}]: {}. Latest claim [{agent}, Round {}]: {}.",
            opening.round, opening_summary, latest.round, latest_summary
        );
        if would_exceed_budget(char_budget, &section, META_MAX_CHARS) {
            sections.push("Additional participant detail omitted for length; transcript evidence remains the source of truth.".into());
            break;
        }
        char_budget += section.len() + 2;
        sections.push(section);
    }

    let mut final_round_entries: Vec<&GroundingEvidenceEntry> = evidence
        .iter()
        .filter(|e| e.round == max_round && !e.abstained && !e.effective_abstained)
        .collect();
    final_round_entries.sort_by(|a, b| a.agent.cmp(&b.agent));
    if !final_round_entries.is_empty() {
        let final_map = final_round_entries
            .iter()
            .map(|e| {
                let label = alias_map
                    .get(&e.agent)
                    .map(|name| format!("{} ({})", e.agent, name))
                    .unwrap_or_else(|| e.agent.clone());
                format!(
                    "{label} [{agent}, Round {}]: {}",
                    e.round,
                    summarize_for_meta(&e.response, 140),
                    agent = e.agent
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        let final_section = format!("Final-round position map: {final_map}");
        if !would_exceed_budget(char_budget, &final_section, META_MAX_CHARS) {
            sections.push(final_section);
        }
    }

    let joined = sections.join("\n\n");
    format!("Conclusion: {joined}")
}

fn build_behavior_notes(
    participant_map_text: &str,
    transcript_text: &str,
    grounding_evidence_json: &str,
) -> String {
    let mut evidence = parse_grounding_evidence(grounding_evidence_json);
    if evidence.is_empty() {
        evidence = parse_transcript_entries(transcript_text)
            .into_iter()
            .map(|entry| GroundingEvidenceEntry {
                agent: entry.agent,
                round: entry.round,
                abstained: false,
                effective_abstained: false,
                valid: true,
                response: entry.text,
            })
            .collect();
    }
    if evidence.is_empty() {
        return "- No transcript evidence available for behaviour analysis.".into();
    }

    let alias_map = parse_participant_map(participant_map_text);
    let mut grouped: HashMap<String, Vec<GroundingEvidenceEntry>> = HashMap::new();
    for row in evidence {
        grouped.entry(row.agent.clone()).or_default().push(row);
    }
    let mut agents: Vec<String> = grouped.keys().cloned().collect();
    agents.sort();
    let mut lines = Vec::new();
    for agent in agents {
        let mut entries = grouped.get(&agent).cloned().unwrap_or_default();
        entries.sort_by_key(|e| e.round);
        let responded = entries
            .iter()
            .filter(|e| !e.abstained && !e.effective_abstained && e.valid)
            .count();
        let abstained = entries
            .iter()
            .filter(|e| e.abstained || e.effective_abstained)
            .count();
        let invalid = entries.iter().filter(|e| !e.valid).count();
        let label = alias_map
            .get(&agent)
            .map(|name| format!("{agent} ({name})"))
            .unwrap_or(agent);
        lines.push(format!(
            "- {label}: responded={responded}, abstained/effective-abstained={abstained}, invalid={invalid}."
        ));
        // For abstaining bots, surface the bot's own wrapper text from the
        // earliest gap round — this is the actionable diagnostic the bot
        // operator needs to fix the failure (provider error, upstream
        // timeout, rate limit, empty-response, etc.).
        if abstained > 0 {
            if let Some(first_gap) = entries
                .iter()
                .find(|e| e.abstained || e.effective_abstained)
            {
                let signal: String = first_gap
                    .response
                    .trim()
                    .replace('\n', " ")
                    .chars()
                    .take(240)
                    .collect();
                if !signal.is_empty() {
                    lines.push(format!(
                        "  Wrapper signal (first gap, Round {}): \"{}\". Operator: check upstream model availability / API key / rate limits in this bot's wrapper.",
                        first_gap.round, signal
                    ));
                }
            }
        }
    }
    lines.join("\n")
}


#[cfg(test)]
mod tests {
    use super::{compose_structured_meta, run_synthesis};
    use crate::config::ModelsConfig;
    use crate::synthesiser::schema::{
        DisagreementSide, LiveDisagreement, MinorityPosition, SynthesisOutput,
    };
    use std::collections::HashMap;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn run_synthesis_accepts_null_minority_confidence() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "content": serde_json::json!({
                            "topic": "t",
                            "consensus_points": [],
                            "live_disagreements": [],
                            "flagged_capitulations": [],
                            "minority_positions": [
                                {
                                    "bot": "Agent A",
                                    "position": "p",
                                    "key_argument": "k [Agent A, Round 2]",
                                    "confidence": null
                                }
                            ],
                            "confidence_trajectories": { "Agent A": [null, 70, null] },
                            "meta_observations": "m"
                        })
                        .to_string()
                    }
                }
            ]
        });

        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let config = ModelsConfig {
            minimax_api_key: "".into(),
            minimax_model: "M2.7".into(),
            minimax_base_url: "http://example.invalid".into(),
            opus_api_key: "".into(),
            opus_model: "".into(),
            analysis_base_url: "http://localhost:8086".into(),
            analysis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
            analysis_connect_timeout_secs: 5,
            analysis_request_timeout_secs: 60,
            analysis_max_concurrency: 2,
            final_synthesis_base_url: server.uri(),
            final_synthesis_model: "Qwen3.5-122B-A10B-UD-Q5_K_XL".into(),
            final_synthesis_connect_timeout_secs: 10,
            final_synthesis_request_timeout_secs: 300,
            final_synthesis_warmup_enabled: false,
            final_synthesis_warmup_max_attempts: 0,
            final_synthesis_warmup_delay_secs: 1,
            local_synthesis_base_url: server.uri(),
            local_synthesis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
        };

        let result = run_synthesis(
            &config,
            "topic",
            "Agent A = Alice",
            "transcript",
            "{}",
            "[]",
            "[]",
            None,
            0.0,
        )
        .await;
        assert!(result.is_ok(), "expected synthesis success, got {result:?}");
    }

    #[tokio::test]
    async fn wait_for_final_synthesis_ready_uses_final_endpoint_and_model() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(body_string_contains(
                "\"model\":\"Qwen3.5-122B-A10B-UD-Q5_K_XL\"",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [
                    { "message": { "content": "{\"ready\":true}" } }
                ]
            })))
            .mount(&server)
            .await;
        let config = ModelsConfig {
            minimax_api_key: "".into(),
            minimax_model: "M2.7".into(),
            minimax_base_url: "http://example.invalid".into(),
            opus_api_key: "".into(),
            opus_model: "".into(),
            analysis_base_url: "http://localhost:8086".into(),
            analysis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
            analysis_connect_timeout_secs: 5,
            analysis_request_timeout_secs: 60,
            analysis_max_concurrency: 2,
            final_synthesis_base_url: server.uri(),
            final_synthesis_model: "Qwen3.5-122B-A10B-UD-Q5_K_XL".into(),
            final_synthesis_connect_timeout_secs: 10,
            final_synthesis_request_timeout_secs: 300,
            final_synthesis_warmup_enabled: true,
            final_synthesis_warmup_max_attempts: 1,
            final_synthesis_warmup_delay_secs: 1,
            local_synthesis_base_url: "http://localhost:8086".into(),
            local_synthesis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
        };

        let report = super::wait_for_final_synthesis_ready(&config).await;
        assert!(report.succeeded, "warmup should succeed, got {report:?}");
        assert_eq!(report.attempts, 1);
    }

    #[tokio::test]
    async fn wait_for_final_synthesis_ready_exhaustion_returns_unsuccessful() {
        let config = ModelsConfig {
            minimax_api_key: "".into(),
            minimax_model: "M2.7".into(),
            minimax_base_url: "http://example.invalid".into(),
            opus_api_key: "".into(),
            opus_model: "".into(),
            analysis_base_url: "http://127.0.0.1:8086".into(),
            analysis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
            analysis_connect_timeout_secs: 1,
            analysis_request_timeout_secs: 2,
            analysis_max_concurrency: 2,
            final_synthesis_base_url: "http://127.0.0.1:1".into(),
            final_synthesis_model: "unreachable-model".into(),
            final_synthesis_connect_timeout_secs: 1,
            final_synthesis_request_timeout_secs: 2,
            final_synthesis_warmup_enabled: true,
            final_synthesis_warmup_max_attempts: 2,
            final_synthesis_warmup_delay_secs: 0,
            local_synthesis_base_url: "http://127.0.0.1:8086".into(),
            local_synthesis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
        };

        let report = super::wait_for_final_synthesis_ready(&config).await;
        assert!(!report.succeeded);
        assert_eq!(report.attempts, 2);
    }

    #[test]
    fn derive_position_narrative_prefers_structured_grounding_evidence() {
        let transcript = "[Agent A, Round 1]: good response\n[Agent B, Round 1]: good response";
        let evidence = serde_json::json!([
            {
                "agent": "Agent A",
                "round": 0,
                "abstained": false,
                "valid": true,
                "response": "R0 position."
            },
            {
                "agent": "Agent A",
                "round": 1,
                "abstained": false,
                "valid": true,
                "response": "R1 position with citation-like text\\n[Agent B, Round 1]: not a marker."
            },
            {
                "agent": "Agent A",
                "round": 2,
                "abstained": false,
                "valid": true,
                "response": "R2 closing position."
            }
        ])
        .to_string();

        let narrative =
            super::derive_position_narrative("Agent A = Jamie-LQClaw", transcript, &evidence);
        assert!(
            narrative.contains("R1=responded"),
            "expected round ledger in narrative: {narrative}"
        );
        assert!(
            !narrative.contains("R1=abstained"),
            "unexpected abstention claim: {narrative}"
        );
        assert!(
            narrative.contains("[Agent A, Round 0]"),
            "expected opening citation: {narrative}"
        );
        assert!(
            narrative.contains("[Agent A, Round 2]"),
            "expected latest citation: {narrative}"
        );
    }

    #[test]
    fn derive_position_narrative_tracks_explicit_abstentions_from_evidence() {
        let evidence = serde_json::json!([
            {
                "agent": "Agent B",
                "round": 0,
                "abstained": false,
                "valid": true,
                "response": "Opening."
            },
            {
                "agent": "Agent B",
                "round": 1,
                "abstained": true,
                "valid": true,
                "response": "(abstained)"
            }
        ])
        .to_string();
        let narrative = super::derive_position_narrative("", "", &evidence);
        assert!(
            narrative.contains("R1=abstained"),
            "expected abstention to be grounded in evidence: {narrative}"
        );
    }

    #[test]
    fn derive_position_narrative_treats_effective_abstention_as_non_response() {
        let evidence = serde_json::json!([
            {
                "agent": "Agent C",
                "round": 0,
                "abstained": false,
                "effective_abstained": false,
                "valid": true,
                "response": "Opening substantive position."
            },
            {
                "agent": "Agent C",
                "round": 1,
                "abstained": false,
                "effective_abstained": true,
                "valid": true,
                "response": "I was unable to formulate a response."
            }
        ])
        .to_string();
        let narrative = super::derive_position_narrative("", "", &evidence);
        assert!(
            narrative.contains("R1=effective-abstention"),
            "expected effective abstention marker: {narrative}"
        );
        assert!(
            !narrative.contains("[Agent C, Round 1]: I was unable to formulate a response."),
            "effective abstention should not be treated as substantive claim: {narrative}"
        );
    }

    #[test]
    fn structured_meta_leads_with_summary_sections() {
        let mut trajectories = HashMap::new();
        trajectories.insert("Agent A".to_string(), vec![Some(70), Some(72), Some(75)]);
        let synthesis = SynthesisOutput {
            topic: "t".into(),
            executive_summary: String::new(),
            consensus_points: vec![],
            live_disagreements: vec![LiveDisagreement {
                issue: "Whether identity certificates improve trust".into(),
                side_a: DisagreementSide {
                    position: "Certificates materially improve trust".into(),
                    headline: "Certificates improve trust".into(),
                    bots: vec!["Agent A".into()],
                    best_argument:
                        "Trust improves when attestations are verifiable [Agent A, Round 2]".into(),
                },
                side_b: DisagreementSide {
                    position: "Certificates do not address root accountability gaps".into(),
                    headline: "Accountability gaps remain".into(),
                    bots: vec!["Agent B".into()],
                    best_argument:
                        "Operational controls matter more than identity badges [Agent B, Round 2]"
                            .into(),
                },
            }],
            flagged_capitulations: vec![],
            minority_positions: vec![MinorityPosition {
                bot: "Agent C".into(),
                position: "Keep identity optional and audit controls mandatory".into(),
                headline: "Audit over identity".into(),
                key_argument:
                    "Mandatory identity can be theatre without enforcement [Agent C, Round 2]"
                        .into(),
                confidence: Some(62),
            }],
            confidence_trajectories: trajectories,
            meta_observations: String::new(),
        };
        let evidence = serde_json::json!([
            {"agent":"Agent A","round":0,"abstained":false,"valid":true,"response":"opening"},
            {"agent":"Agent B","round":0,"abstained":false,"valid":true,"response":"opening"},
            {"agent":"Agent C","round":0,"abstained":true,"valid":true,"response":"(abstained)"}
        ])
        .to_string();

        let meta =
            compose_structured_meta(&synthesis, "Agent A = Alice\nAgent B = Bob", "", &evidence);
        assert!(meta.starts_with("Conclusion:"));
        assert!(meta.contains("Summary of arguments:"));
        assert!(meta.contains("Key disagreements:"));
        assert!(meta.contains("Minority positions:"));
        assert!(meta.contains("Overall outcome:"));
        assert!(meta.contains("Bot behaviour notes:"));
    }
}
