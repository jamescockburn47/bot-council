pub mod challenge;
pub mod divergence;
pub mod pairing;

use serde::{Deserialize, Serialize};
use crate::config::ModelsConfig;

/// A generic MiniMax chat completion request.
#[derive(Debug, Serialize)]
struct MiniMaxRequest {
    model: String,
    messages: Vec<MiniMaxMessage>,
    temperature: f64,
    response_format: Option<ResponseFormat>,
}

/// A single message in a MiniMax chat completion request.
#[derive(Debug, Serialize)]
struct MiniMaxMessage {
    role: String,
    content: String,
}

/// Response format specifier for structured output.
#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

/// Top-level response from the MiniMax API.
#[derive(Debug, Deserialize)]
struct MiniMaxResponse {
    choices: Vec<MiniMaxChoice>,
}

/// A single completion choice from MiniMax.
#[derive(Debug, Deserialize)]
struct MiniMaxChoice {
    message: MiniMaxChoiceMessage,
}

/// The message body within a MiniMax choice.
#[derive(Debug, Deserialize)]
struct MiniMaxChoiceMessage {
    content: String,
}

/// Maximum retries for transient MiniMax errors (529 overloaded).
const MINIMAX_MAX_RETRIES: u32 = 3;

/// Call MiniMax with a system prompt and expect a JSON response string.
///
/// Retries on 529 (overloaded) up to `MINIMAX_MAX_RETRIES` times with
/// exponential backoff. Returns the cleaned JSON content string from the
/// first choice, or an error describing what went wrong.
pub async fn call_minimax(
    config: &ModelsConfig,
    system_prompt: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/chat/completions", config.minimax_base_url);

    let request = MiniMaxRequest {
        model: config.minimax_model.clone(),
        messages: vec![
            MiniMaxMessage { role: "user".into(), content: system_prompt.into() },
        ],
        temperature: 0.0,
        response_format: Some(ResponseFormat { format_type: "json_object".into() }),
    };

    let mut last_err = String::new();
    for attempt in 0..=MINIMAX_MAX_RETRIES {
        if attempt > 0 {
            let delay = std::time::Duration::from_secs(2u64.pow(attempt));
            tokio::time::sleep(delay).await;
        }

        let resp = match client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.minimax_api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => { last_err = format!("MiniMax request failed: {e}"); continue; }
        };

        let status = resp.status();
        if status.as_u16() == 529 {
            let body = resp.text().await.unwrap_or_default();
            last_err = format!("MiniMax returned HTTP 529: {body}");
            tracing::warn!(attempt, "MiniMax overloaded (529), retrying");
            continue;
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("MiniMax returned HTTP {status}: {body}"));
        }

        let parsed: MiniMaxResponse = resp.json()
            .await
            .map_err(|e| format!("MiniMax response parse failed: {e}"))?;

        return parsed.choices.first()
            .map(|c| clean_model_output(&c.message.content))
            .ok_or_else(|| "MiniMax returned empty choices".into());
    }

    Err(last_err)
}

/// Clean raw MiniMax M2.7 output to extract JSON content.
///
/// Handles two quirks:
/// 1. `<think>…</think>` reasoning tags wrapped around the real content
/// 2. Markdown code fences (` ```json … ``` `) around JSON
fn clean_model_output(content: &str) -> String {
    let mut s = content.trim().to_string();

    // Strip <think>…</think> tags
    if s.starts_with("<think>") {
        if let Some(pos) = s.find("</think>") {
            s = s[pos + 8..].trim().to_string();
        }
    }

    // Strip markdown code fences
    if s.starts_with("```") {
        // Remove opening fence (with optional language hint)
        if let Some(newline) = s.find('\n') {
            s = s[newline + 1..].to_string();
        }
        // Remove closing fence
        if let Some(pos) = s.rfind("```") {
            s = s[..pos].trim().to_string();
        }
    }

    s
}
