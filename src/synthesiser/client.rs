//! OpenAI-compatible chat client for the final-synthesis model.

use crate::config::ModelsConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Retry count for local synthesis model calls.
const LOCAL_SYNTHESIS_MAX_RETRIES: u32 = 3;

/// Serialisable OpenAI-compatible chat request body.
#[derive(Debug, Serialize)]
pub(crate) struct LocalChatCompletionRequest {
    pub model: String,
    pub temperature: f64,
    pub top_k: i32,
    pub seed: u32,
    pub cache_prompt: bool,
    pub reasoning_format: String,
    pub response_format: LocalResponseFormat,
    pub messages: Vec<LocalChatMessage>,
}

/// A single message in the local chat request.
#[derive(Debug, Serialize)]
pub(crate) struct LocalChatMessage {
    pub role: String,
    pub content: String,
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
pub(crate) struct LocalResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<serde_json::Value>,
}

/// Call the local synthesis model on EVO.
pub(crate) async fn call_local_synthesis_model(
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

pub(crate) async fn call_model_json(
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
pub(crate) fn extract_json_object(text: &str) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::extract_json_object;

    #[test]
    fn extract_json_object_handles_channel_wrapped_output() {
        let raw = r#"<|channel>thought
<channel|>```json
{"topic":"t","headline":"h","executive_summary":"e","issues":[],"meta_observations":"m"}
```"#;
        let extracted = extract_json_object(raw).expect("should extract JSON");
        let value: serde_json::Value = serde_json::from_str(&extracted).expect("valid json");
        assert_eq!(value.get("topic").and_then(|v| v.as_str()), Some("t"));
    }
}
