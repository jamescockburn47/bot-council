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

/// Call MiniMax with a system prompt and expect a JSON response string.
///
/// Returns the raw JSON content string from the first choice, or an error
/// describing what went wrong (network failure, non-2xx status, empty choices).
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

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.minimax_api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("MiniMax request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default(); // intentional: error body is best-effort
        return Err(format!("MiniMax returned HTTP {status}: {body}"));
    }

    let parsed: MiniMaxResponse = resp.json()
        .await
        .map_err(|e| format!("MiniMax response parse failed: {e}"))?;

    parsed.choices.first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| "MiniMax returned empty choices".into())
}
