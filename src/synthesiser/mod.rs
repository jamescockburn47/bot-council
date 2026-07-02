/// LLM-backed effective-abstention classifier (synthesis prep).
pub mod abstention_classifier;
/// Post-synthesis citation validation.
pub mod citation_check;
/// OpenAI-compatible chat client for the final-synthesis model.
mod client;
/// Transcript / grounding-evidence parsing shared across the module.
mod evidence;
/// Deterministic meta_observations composition.
mod meta;
/// Pre-computation of structural debate data.
pub mod precompute;
/// Synthesis prompt construction.
mod prompt;
/// Output schema for the synthesis result.
pub mod schema;

use crate::analyser::crux::CruxSelection;
use crate::config::ModelsConfig;
use crate::synthesiser::schema::SessionArtifact;
use client::{
    LocalChatCompletionRequest, LocalChatMessage, LocalResponseFormat, call_local_synthesis_model,
    call_model_json,
};
use evidence::{
    GroundingEvidenceEntry, parse_grounding_evidence, parse_transcript_entries, summarize_for_meta,
};
use prompt::build_synthesis_prompt;
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
    // an empty issues array, which triggers our salvage path and writes a
    // vapid stub. Empirically an immediate retry — often with slightly
    // bumped temperature — produces correct content. We attempt up to 3
    // times before accepting the empty result (which itself still has the
    // structural-fallback safety net downstream).
    let transcript_has_substance = transcript_text.trim().len() > 500;
    let max_attempts: u32 = 3;
    let mut parsed: Option<SessionArtifact> = None;
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
                let mut fallback = conservative_fallback(topic);
                meta::ensure_substantive_meta(
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
                match serde_json::from_value::<SessionArtifact>(value.clone()) {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(error = %e, attempt, "non-schema JSON after topic-reinject; salvaging");
                        salvage_loose_output(
                            topic,
                            participant_map_text,
                            transcript_text,
                            grounding_evidence_json,
                            &value,
                        )
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, attempt, "unparseable JSON; using fallback");
                conservative_fallback(topic)
            }
        };

        let is_structurally_empty = attempt_parsed.issues.is_empty();
        if is_structurally_empty && transcript_has_substance && attempt < max_attempts {
            tracing::warn!(
                attempt,
                next_temperature = (temperature + 0.1 * f64::from(attempt)).min(0.5),
                "synthesis returned an empty issue map despite substantive transcript; retrying"
            );
            continue;
        }

        parsed = Some(attempt_parsed);
        break;
    }
    let mut parsed =
        parsed.expect("retry loop must either return early or produce a SessionArtifact");
    // Hardening: if the model returned an empty issue map but the
    // transcript contains substantive non-abstained content, emit one
    // fallback issue holding each bot's latest position so the downstream
    // argument map still has nodes. Prompt changes already push the model
    // toward populated output; this is the last-resort safety net.
    enrich_empty_output_with_structural_fallback(
        &mut parsed,
        transcript_text,
        grounding_evidence_json,
    );
    ensure_crux_issue(&mut parsed, crux);
    meta::ensure_substantive_meta(
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

/// Last-resort hardening: when the model returns an empty issue map but the
/// transcript has substantive non-abstained content, emit one Split issue
/// holding each agent's latest substantive position. Ensures the map has
/// nodes even when extraction fails. No-op if `issues` is non-empty.
fn enrich_empty_output_with_structural_fallback(
    output: &mut SessionArtifact,
    transcript_text: &str,
    grounding_evidence_json: &str,
) {
    if !output.issues.is_empty() {
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

    let mut positions = Vec::new();
    for agent in agents {
        let entry = &latest_by_agent[&agent];
        positions.push(schema::Position {
            stance: summarize_for_meta(&entry.response, 280),
            headline: format!("{} position (R{})", agent, entry.round),
            bots: vec![agent.clone()],
            best_argument: format!(
                "Derived from [{}, Round {}] — no structured argument extracted by the synthesiser.",
                agent, entry.round
            ),
            evidence: String::new(),
            final_confidence: None,
            frame_rejection: false,
        });
    }
    output.issues.push(schema::Issue {
        issue: "Final positions on record (structural fallback — the synthesiser extracted no issue map)".into(),
        headline: "Positions on record".into(),
        is_crux: false,
        status: schema::IssueStatus::Split,
        positions,
        movement: Vec::new(),
    });

    tracing::info!("synthesis: empty output enriched with structural fallback issue");
}

/// Spec guarantee: when crux selection succeeded, the artifact carries
/// exactly one `is_crux` issue. If the model forgot the flag, mark the
/// issue with the highest word-overlap against the crux claim; if the
/// issue map is empty (or nothing overlaps), inject a positions-empty
/// crux issue so the reader still sees what the debate turned on.
fn ensure_crux_issue(output: &mut SessionArtifact, crux: Option<&CruxSelection>) {
    let Some(crux) = crux else { return };
    if output.issues.iter().any(|i| i.is_crux) {
        return;
    }
    fn words_of(s: &str) -> std::collections::HashSet<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 3)
            .map(str::to_string)
            .collect()
    }
    let claim_words = words_of(&crux.claim);
    let best = output
        .issues
        .iter_mut()
        .map(|i| {
            let overlap = words_of(&i.issue).intersection(&claim_words).count();
            (overlap, i)
        })
        .max_by_key(|(overlap, _)| *overlap);
    match best {
        Some((overlap, issue)) if overlap >= 2 => issue.is_crux = true,
        _ => output.issues.push(schema::Issue {
            issue: crux.claim.clone(),
            headline: String::new(),
            is_crux: true,
            status: schema::IssueStatus::Split,
            positions: Vec::new(),
            movement: Vec::new(),
        }),
    }
}

/// Build a deterministic no-hallucination fallback with an empty issue map.
fn conservative_fallback(topic: &str) -> SessionArtifact {
    SessionArtifact {
        topic: topic.to_string(),
        headline: String::new(),
        executive_summary: String::new(),
        issues: Vec::new(),
        meta_observations: "Conservative fallback synthesis: no structured issue map is reported because the synthesis model output could not be validated against the required schema.".into(),
    }
}

/// Convert non-conforming model JSON into safe synthesis output.
fn salvage_loose_output(
    topic: &str,
    participant_map_text: &str,
    transcript_text: &str,
    grounding_evidence_json: &str,
    loose: &serde_json::Value,
) -> SessionArtifact {
    let mut output = conservative_fallback(topic);
    if let Some(loose_topic) = loose.get("topic").and_then(|v| v.as_str()) {
        if !loose_topic.trim().is_empty() {
            output.topic = loose_topic.to_string();
        }
    }
    if let Some(h) = loose.get("headline").and_then(|v| v.as_str()) {
        if !h.trim().is_empty() {
            output.headline = h.trim().to_string();
        }
    }
    if let Some(summary) = loose.get("executive_summary").and_then(|v| v.as_str()) {
        if !summary.trim().is_empty() {
            output.executive_summary = summary.trim().to_string();
        }
    }
    // Issues salvage: serde defaults make partially-shaped issue arrays
    // recoverable even when the top-level object failed the typed parse.
    if let Some(issues_val) = loose.get("issues") {
        if let Ok(issues) = serde_json::from_value::<Vec<schema::Issue>>(issues_val.clone()) {
            output.issues = issues;
        }
    }
    if let Some(meta_str) = loose
        .get("meta_observations")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        output.meta_observations = meta_str.to_string();
    }
    meta::ensure_substantive_meta(
        &mut output,
        participant_map_text,
        transcript_text,
        grounding_evidence_json,
    );
    output
}

#[cfg(test)]
mod tests {
    use super::run_synthesis;
    use crate::config::ModelsConfig;
    use crate::synthesiser::schema::{Issue, IssueStatus, SessionArtifact};
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn run_synthesis_accepts_null_position_confidence() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "content": serde_json::json!({
                            "topic": "t",
                            "headline": "One issue, one position.",
                            "executive_summary": "One. Two. Three. Four.",
                            "issues": [{
                                "issue": "q",
                                "headline": "Question label",
                                "is_crux": false,
                                "status": "split",
                                "positions": [{
                                    "stance": "p",
                                    "headline": "Stance label",
                                    "bots": ["Agent A"],
                                    "best_argument": "k [Agent A, Round 2]",
                                    "evidence": "",
                                    "final_confidence": null,
                                    "frame_rejection": false
                                }],
                                "movement": []
                            }],
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
    fn ensure_crux_issue_marks_best_overlap() {
        let crux = crate::analyser::crux::CruxSelection {
            claim: "Whether enforcement should be ex-ante rather than ex-post".into(),
            source_pseudonym: "Agent A".into(),
            source_quote: "q".into(),
        };
        let mut artifact = SessionArtifact {
            topic: "t".into(),
            headline: String::new(),
            executive_summary: String::new(),
            issues: vec![
                Issue {
                    issue: "Whether capture risk is real".into(),
                    headline: String::new(),
                    is_crux: false,
                    status: IssueStatus::Settled,
                    positions: vec![],
                    movement: vec![],
                },
                Issue {
                    issue: "Whether enforcement should be ex-ante".into(),
                    headline: String::new(),
                    is_crux: false,
                    status: IssueStatus::Split,
                    positions: vec![],
                    movement: vec![],
                },
            ],
            meta_observations: String::new(),
        };
        super::ensure_crux_issue(&mut artifact, Some(&crux));
        assert!(!artifact.issues[0].is_crux);
        assert!(artifact.issues[1].is_crux);
    }

    #[test]
    fn ensure_crux_issue_injects_when_no_overlap() {
        let crux = crate::analyser::crux::CruxSelection {
            claim: "Completely unrelated crux claim".into(),
            source_pseudonym: "Agent A".into(),
            source_quote: "q".into(),
        };
        let mut artifact = SessionArtifact {
            topic: "t".into(),
            headline: String::new(),
            executive_summary: String::new(),
            issues: vec![],
            meta_observations: String::new(),
        };
        super::ensure_crux_issue(&mut artifact, Some(&crux));
        assert_eq!(artifact.issues.len(), 1);
        assert!(artifact.issues[0].is_crux);
        assert_eq!(artifact.issues[0].issue, "Completely unrelated crux claim");
    }
}
