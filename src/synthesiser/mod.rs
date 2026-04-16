/// Post-synthesis citation validation.
pub mod citation_check;
/// Pre-computation of structural debate data.
pub mod precompute;
/// Output schema for the synthesis result.
pub mod schema;

use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use crate::config::ModelsConfig;
use crate::sanitise::ANTI_INJECTION_PREAMBLE;

/// Call Opus to produce the final synthesis.
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
/// * `temperature` — sampling temperature for the Opus call
pub async fn run_synthesis(
    config: &ModelsConfig,
    topic: &str,
    transcript_text: &str,
    precomputed_json: &str,
    divergence_results_json: &str,
    temperature: f64,
) -> Result<(String, String), String> {
    let system_prompt = build_synthesis_prompt(
        topic,
        transcript_text,
        precomputed_json,
        divergence_results_json,
    );

    let prompt_hash = {
        let mut hasher = Sha256::new();
        hasher.update(system_prompt.as_bytes());
        hex::encode(hasher.finalize())
    };

    let client = reqwest::Client::new();
    let request = AnthropicRequest {
        model: config.opus_model.clone(),
        max_tokens: 4096,
        temperature,
        messages: vec![
            AnthropicMessage { role: "user".into(), content: system_prompt },
        ],
    };

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &config.opus_api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Opus request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Opus returned HTTP {status}: {body}"));
    }

    let parsed: AnthropicResponse = resp
        .json()
        .await
        .map_err(|e| format!("Opus response parse failed: {e}"))?;

    let content = parsed
        .content
        .into_iter()
        .next()
        .map(|c| c.text)
        .ok_or_else(|| "Opus returned empty content".to_string())?;

    Ok((content, prompt_hash))
}

/// Build the full synthesis prompt from debate artifacts.
fn build_synthesis_prompt(
    topic: &str,
    transcript: &str,
    precomputed: &str,
    divergence: &str,
) -> String {
    format!(
        "You are the synthesis engine for a structured adversarial debate. \
         Your role is analytical, not creative. You must produce a rigorous, citation-backed synthesis.\n\n\
         {ANTI_INJECTION_PREAMBLE}\n\n\
         RULES:\n\
         - Every factual claim must cite [Bot pseudonym, Round N].\n\
         - Do not infer what a participant \"seemed to mean\" — use only their stated positions.\n\
         - Do not declare consensus unless all participants explicitly agree on the specific point.\n\
         - Preserve minority positions with full dignity.\n\
         - Flag any position shift that lacks adequate justification.\n\n\
         TOPIC: {topic}\n\n\
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
           \"meta_observations\": \"string — max 200 words\"\n\
         }}"
    )
}

/// Serialisable request body for the Anthropic Messages API.
#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    temperature: f64,
    messages: Vec<AnthropicMessage>,
}

/// A single message in the Anthropic request.
#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

/// Top-level response from the Anthropic Messages API.
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

/// A single content block in the Anthropic response.
#[derive(Debug, Deserialize)]
struct AnthropicContent {
    text: String,
}
