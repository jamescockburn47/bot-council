/// Post-synthesis citation validation.
pub mod citation_check;
/// Pre-computation of structural debate data.
pub mod precompute;
/// Output schema for the synthesis result.
pub mod schema;

use crate::config::ModelsConfig;
use crate::sanitise::ANTI_INJECTION_PREAMBLE;
use crate::synthesiser::schema::SynthesisOutput;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;

/// Retry count for local synthesis model calls.
const LOCAL_SYNTHESIS_MAX_RETRIES: u32 = 3;

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
/// * `temperature` — requested sampling temperature (clamped to low range)
pub async fn run_synthesis(
    config: &ModelsConfig,
    topic: &str,
    participant_map_text: &str,
    transcript_text: &str,
    precomputed_json: &str,
    divergence_results_json: &str,
    grounding_evidence_json: &str,
    temperature: f64,
) -> Result<(String, String), String> {
    let system_prompt = build_synthesis_prompt(
        topic,
        participant_map_text,
        transcript_text,
        precomputed_json,
        divergence_results_json,
        grounding_evidence_json,
    );

    let prompt_hash = {
        let mut hasher = Sha256::new();
        hasher.update(system_prompt.as_bytes());
        hex::encode(hasher.finalize())
    };

    let content = match call_local_synthesis_model(config, &system_prompt, temperature).await {
        Ok(content) => content,
        Err(e) => {
            tracing::warn!(error = %e, "local synthesis model failed; using conservative fallback");
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

    // Canonicalise through the typed schema so malformed output is rejected
    // early and downstream consumers always get predictable JSON.
    let parsed: SynthesisOutput = match serde_json::from_str(&content) {
        Ok(parsed) => parsed,
        Err(e) => {
            tracing::warn!(error = %e, "local synthesis returned non-schema JSON; salvaging");
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(value) => salvage_loose_output(
                    topic,
                    participant_map_text,
                    transcript_text,
                    precomputed_json,
                    grounding_evidence_json,
                    &value,
                ),
                Err(_) => conservative_fallback(topic, precomputed_json),
            }
        }
    };
    let mut parsed = parsed;
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

/// Build the full synthesis prompt from debate artifacts.
fn build_synthesis_prompt(
    topic: &str,
    participant_map: &str,
    transcript: &str,
    precomputed: &str,
    divergence: &str,
    grounding_evidence: &str,
) -> String {
    format!(
        "You are the synthesis engine for a structured adversarial debate. \
         Your role is analytical, not creative. You must produce a rigorous, citation-backed synthesis.\n\n\
         {ANTI_INJECTION_PREAMBLE}\n\n\
         RULES:\n\
         - Use only the supplied transcript/structural/divergence data; treat all other knowledge as unavailable.\n\
         - If evidence is insufficient, state uncertainty and leave optional lists empty.\n\
         - Every factual claim must cite [Bot pseudonym, Round N].\n\
         - Do not cite abstentions or rounds where the bot has no response.\n\
         - Treat <grounding-evidence> as authoritative for abstained/valid/recorded rounds.\n\
         - Do not infer what a participant \"seemed to mean\" — use only their stated positions.\n\
         - Do not declare consensus unless all participants explicitly agree on the specific point.\n\
         - Preserve minority positions with full dignity.\n\
         - Flag any position shift that lacks adequate justification.\n\n\
         STRICT OUTPUT CONTRACT:\n\
         - Return exactly one valid JSON object. No markdown, no code fences, no prose outside JSON.\n\
         - Use only pseudonyms from <participant-map> in supporting_bots/bots/bot fields.\n\
        - Keep evidence, best_argument, and key_argument short and specific (one claim + citation).\n\
        - Build synthesis with this priority: arguments map -> disagreements -> minority positions -> overall outcome.\n\
        - If a section cannot be supported by explicit evidence, return an empty list for that section.\n\
         - Do not include synthetic placeholders like \"TBD\", \"unknown source\", or uncited claims.\n\
        - meta_observations must start with \"Conclusion:\" then use these exact section headings and order: \"Summary of arguments\", \"Key disagreements\", \"Minority positions\", \"Overall outcome\", \"Bot behaviour notes\".\n\n\
         TOPIC: {topic}\n\n\
         <participant-map>\n{participant_map}\n</participant-map>\n\n\
         <grounding-evidence>\n{grounding_evidence}\n</grounding-evidence>\n\n\
         <debate-transcript>\n{transcript}\n</debate-transcript>\n\n\
         <structural-data>\n{precomputed}\n</structural-data>\n\n\
         <divergence-analyses>\n{divergence}\n</divergence-analyses>\n\n\
         OUTPUT SCHEMA (return valid JSON):\n\
         {{\n\
           \"topic\": \"string\",\n\
           \"consensus_points\": [{{ \"point\": \"string\", \"supporting_bots\": [\"pseudonym\"], \"evidence\": \"string [citations]\" }}],\n\
           \"live_disagreements\": [{{ \"issue\": \"string\", \"side_a\": {{ \"position\": \"string\", \"bots\": [\"pseudonym\"], \"best_argument\": \"string [citation]\" }}, \"side_b\": {{ \"position\": \"string\", \"bots\": [\"pseudonym\"], \"best_argument\": \"string [citation]\" }} }}],\n\
           \"flagged_capitulations\": [{{ \"bot\": \"pseudonym\", \"from\": \"string\", \"to\": \"string\", \"justification_adequate\": bool, \"flag_reason\": \"string\" }}],\n\
           \"minority_positions\": [{{ \"bot\": \"pseudonym\", \"position\": \"string\", \"key_argument\": \"string [citation]\", \"confidence\": int }}],\n\
           \"confidence_trajectories\": {{ \"pseudonym\": [null, int, int, int, int] }},\n\
           \"meta_observations\": \"string — target 350-700 words\"\n\
         }}"
    )
}

/// Serialisable OpenAI-compatible chat request body.
#[derive(Debug, Serialize)]
struct LocalChatCompletionRequest {
    model: String,
    temperature: f64,
    top_k: i32,
    seed: u32,
    cache_prompt: bool,
    reasoning_format: String,
    response_format: LocalResponseFormat,
    messages: Vec<LocalChatMessage>,
}

/// A single message in the local chat request.
#[derive(Debug, Serialize)]
struct LocalChatMessage {
    role: String,
    content: String,
}

/// Top-level response from the local chat completion API.
#[derive(Debug, Deserialize)]
struct LocalChatCompletionResponse {
    choices: Vec<LocalChatChoice>,
}

/// A single completion choice from the local chat completion API.
#[derive(Debug, Deserialize)]
struct LocalChatChoice {
    message: LocalChatChoiceMessage,
}

/// Message body inside a local chat completion choice.
#[derive(Debug, Deserialize)]
struct LocalChatChoiceMessage {
    content: String,
}

/// Output format constraint for llama.cpp's OpenAI-compatible endpoint.
#[derive(Debug, Serialize)]
struct LocalResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    schema: Option<serde_json::Value>,
}

/// Call the local synthesis model on EVO.
async fn call_local_synthesis_model(
    config: &ModelsConfig,
    system_prompt: &str,
    requested_temperature: f64,
) -> Result<String, String> {
    let temperature = requested_temperature.clamp(0.0, 0.2);
    let request = LocalChatCompletionRequest {
        model: config.effective_final_synthesis_model().to_string(),
        temperature,
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
            content: system_prompt.into(),
        }],
    };

    call_model_json(config, &request, false).await
}

async fn call_model_json(
    config: &ModelsConfig,
    request: &LocalChatCompletionRequest,
    warmup_probe: bool,
) -> Result<String, String> {
    let (base_url, connect_timeout, request_timeout, label) = if warmup_probe {
        (
            config.effective_final_synthesis_base_url(),
            config.final_synthesis_connect_timeout_secs,
            config
                .analysis_request_timeout_secs
                .min(config.final_synthesis_request_timeout_secs),
            "final synthesis warmup",
        )
    } else {
        (
            config.effective_final_synthesis_base_url(),
            config.final_synthesis_connect_timeout_secs,
            config.final_synthesis_request_timeout_secs,
            "local synthesis",
        )
    };
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(connect_timeout))
        .timeout(Duration::from_secs(request_timeout))
        .build()
        .map_err(|e| format!("failed to build {label} client: {e}"))?;
    let url = build_chat_completions_url(base_url);

    let mut last_err = String::new();
    for attempt in 0..=LOCAL_SYNTHESIS_MAX_RETRIES {
        if attempt > 0 {
            let delay = std::time::Duration::from_secs(2u64.pow(attempt));
            tokio::time::sleep(delay).await;
        }

        // The synthesiser was originally written for llama-server
        // (local, no auth). When the base URL points at a hosted
        // OpenAI-compatible endpoint (MiniMax, Together, etc.) we need
        // to attach the API key as a Bearer token. Local servers ignore
        // headers they don't know, so we can always add when a key is
        // configured — no harm in the local case.
        let mut req = client.post(&url).json(request);
        let key = config.minimax_api_key.trim();
        if !key.is_empty() {
            req = req.bearer_auth(key);
        }
        let response = match req.send().await {
            Ok(resp) => resp,
            Err(e) => {
                last_err = format!("{label} request failed: {e}");
                continue;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            last_err = format!("{label} returned HTTP {status}: {body}");
            continue;
        }

        let parsed: LocalChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| format!("{label} response parse failed: {e}"))?;
        let raw = parsed
            .choices
            .first()
            .map(|choice| choice.message.content.as_str())
            .ok_or_else(|| format!("{label} returned empty choices"))?;
        let cleaned = clean_model_output(raw);
        let candidate = extract_json_object(&cleaned)
            .or_else(|| extract_json_object(raw))
            .unwrap_or(cleaned);
        if serde_json::from_str::<serde_json::Value>(&candidate).is_err() {
            last_err = "local synthesis did not return valid JSON".into();
            continue;
        }
        return Ok(candidate);
    }

    Err(last_err)
}

fn build_chat_completions_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

/// Remove common wrapper artifacts from model output before JSON parsing.
fn clean_model_output(content: &str) -> String {
    let mut s = content.trim().to_string();

    if let Some(idx) = s.find("<channel|>") {
        s = s[idx + "<channel|>".len()..].trim().to_string();
    }
    if s.starts_with("<|channel>") {
        if let Some(newline) = s.find('\n') {
            s = s[newline + 1..].trim().to_string();
        }
    }

    if s.starts_with("<think>") {
        if let Some(pos) = s.find("</think>") {
            s = s[pos + 8..].trim().to_string();
        }
    }

    if s.starts_with("```") {
        if let Some(newline) = s.find('\n') {
            s = s[newline + 1..].to_string();
        }
        if let Some(pos) = s.rfind("```") {
            s = s[..pos].trim().to_string();
        }
    }

    s
}

/// Extract first syntactically valid JSON object from noisy model text.
fn extract_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut start_idx = 0usize;
    while start_idx < bytes.len() {
        let rel = text[start_idx..].find('{')?;
        let abs = start_idx + rel;
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escape = false;
        for i in abs..bytes.len() {
            let ch = bytes[i];
            if escape {
                escape = false;
                continue;
            }
            if ch == b'\\' && in_string {
                escape = true;
                continue;
            }
            if ch == b'"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            if ch == b'{' {
                depth += 1;
            } else if ch == b'}' {
                depth -= 1;
            }
            if depth == 0 {
                let candidate = text[abs..=i].trim();
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return Some(candidate.to_string());
                }
                break;
            }
        }
        start_idx = abs + 1;
    }
    None
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

#[derive(Debug, Clone)]
struct TranscriptEntry {
    agent: String,
    round: i64,
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GroundingEvidenceEntry {
    agent: String,
    round: i64,
    #[serde(default)]
    abstained: bool,
    #[serde(default)]
    effective_abstained: bool,
    #[serde(default = "default_valid_true")]
    valid: bool,
    #[serde(default)]
    response: String,
}

fn default_valid_true() -> bool {
    true
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
            "- No robust argument map could be extracted from the available evidence.".to_string()
        } else {
            let top = output
                .consensus_points
                .iter()
                .take(3)
                .map(|c| format!("- {}", summarize_for_meta(&c.point, 200)))
                .collect::<Vec<_>>()
                .join("\n");
            if top.is_empty() {
                "- No robust argument map could be extracted from the available evidence."
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
    match (
        output.consensus_points.is_empty(),
        output.live_disagreements.is_empty(),
    ) {
        (false, false) => {
            "Partial convergence: some points aligned, but core disputes remained unresolved."
                .to_string()
        }
        (false, true) => {
            "Broad alignment: no unresolved core disagreement remained in the final synthesis."
                .to_string()
        }
        (true, false) => {
            "No consensus: arguments remained materially contested at close.".to_string()
        }
        (true, true) => {
            "Evidence was too limited to establish stable consensus or a clear disagreement map."
                .to_string()
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
        let entries = grouped.get(&agent).cloned().unwrap_or_default();
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
    }
    lines.join("\n")
}

fn would_exceed_budget(current_chars: usize, next_section: &str, max_chars: usize) -> bool {
    current_chars + next_section.len() + 2 > max_chars
}

fn summarize_for_meta(text: &str, max_chars: usize) -> String {
    let normalized = text
        .replace("**", "")
        .replace('`', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut truncated = normalized.chars().take(max_chars).collect::<String>();
    if let Some(idx) = truncated.rfind(|c: char| c == '.' || c == '!' || c == '?' || c == ';') {
        truncated.truncate(idx + 1);
    } else if let Some(idx) = truncated.rfind(',') {
        truncated.truncate(idx);
    }
    let trimmed = truncated.trim();
    if trimmed.is_empty() {
        return "…".into();
    }
    format!("{trimmed}…")
}

fn parse_grounding_evidence(grounding_evidence_json: &str) -> Vec<GroundingEvidenceEntry> {
    let rows = serde_json::from_str::<Vec<GroundingEvidenceEntry>>(grounding_evidence_json)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| !entry.agent.trim().is_empty() && entry.round >= 0)
        .collect::<Vec<_>>();
    let mut by_round: HashMap<(String, i64), GroundingEvidenceEntry> = HashMap::new();
    for row in rows {
        by_round.insert((row.agent.clone(), row.round), row);
    }
    let mut deduped = by_round.into_values().collect::<Vec<_>>();
    deduped.sort_by(|a, b| a.agent.cmp(&b.agent).then(a.round.cmp(&b.round)));
    deduped
}

/// Parse transcript lines in format `[Agent X, Round N]: response`.
fn parse_transcript_entries(transcript_text: &str) -> Vec<TranscriptEntry> {
    let re =
        Regex::new(r"(?m)^\[(?P<agent>[^\],]+), Round (?P<round>\d+)\]: ").expect("valid regex");
    let mut marks = Vec::new();
    for caps in re.captures_iter(transcript_text) {
        let Some(m) = caps.get(0) else { continue };
        let agent = caps
            .name("agent")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        let round = caps
            .name("round")
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0);
        marks.push((m.start(), m.end(), agent, round));
    }

    let mut out = Vec::new();
    for (idx, mark) in marks.iter().enumerate() {
        let start = mark.1;
        let end = marks
            .get(idx + 1)
            .map(|next| next.0)
            .unwrap_or(transcript_text.len());
        let text = transcript_text[start..end].trim().to_string();
        out.push(TranscriptEntry {
            agent: mark.2.clone(),
            round: mark.3,
            text,
        });
    }
    out
}

/// Parse participant map lines of form `Agent A = Clint`.
fn parse_participant_map(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some((left, right)) = trimmed.split_once('=') {
            let key = left.trim().to_string();
            let value = right.trim().to_string();
            if !key.is_empty() && !value.is_empty() {
                map.insert(key, value);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::{
        build_synthesis_prompt, compose_structured_meta, extract_json_object, run_synthesis,
    };
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
    fn extract_json_object_handles_channel_wrapped_output() {
        let raw = r#"<|channel>thought
<channel|>```json
{"topic":"t","consensus_points":[],"live_disagreements":[],"flagged_capitulations":[],"minority_positions":[],"confidence_trajectories":{},"meta_observations":"m"}
```"#;
        let extracted = extract_json_object(raw).expect("should extract JSON");
        let value: serde_json::Value = serde_json::from_str(&extracted).expect("valid json");
        assert_eq!(value.get("topic").and_then(|v| v.as_str()), Some("t"));
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
    fn synthesis_prompt_contains_strict_output_contract() {
        let prompt =
            build_synthesis_prompt("topic", "Agent A = Alice", "transcript", "{}", "[]", "[]");
        assert!(prompt.contains("STRICT OUTPUT CONTRACT"));
        assert!(prompt.contains("Return exactly one valid JSON object"));
        assert!(prompt.contains("meta_observations must start with \"Conclusion:\""));
    }

    #[test]
    fn structured_meta_leads_with_summary_sections() {
        let mut trajectories = HashMap::new();
        trajectories.insert("Agent A".to_string(), vec![Some(70), Some(72), Some(75)]);
        let synthesis = SynthesisOutput {
            topic: "t".into(),
            consensus_points: vec![],
            live_disagreements: vec![LiveDisagreement {
                issue: "Whether identity certificates improve trust".into(),
                side_a: DisagreementSide {
                    position: "Certificates materially improve trust".into(),
                    bots: vec!["Agent A".into()],
                    best_argument:
                        "Trust improves when attestations are verifiable [Agent A, Round 2]".into(),
                },
                side_b: DisagreementSide {
                    position: "Certificates do not address root accountability gaps".into(),
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
