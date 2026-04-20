pub mod challenge;
pub mod divergence;
pub mod pairing;

use crate::config::ModelsConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A generic local chat completion request.
#[derive(Debug, Serialize)]
struct MiniMaxRequest {
    model: String,
    messages: Vec<MiniMaxMessage>,
    temperature: f64,
    top_k: i32,
    seed: u32,
    cache_prompt: bool,
    reasoning_format: String,
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

/// Top-level response from the local chat API.
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

/// Maximum retries for transient local inference errors.
const MINIMAX_MAX_RETRIES: u32 = 3;

/// Call local EVO model with a prompt and expect JSON.
///
/// Retries on transient failures up to `MINIMAX_MAX_RETRIES` times with
/// exponential backoff. Returns the cleaned JSON content string from the
/// first choice, or an error describing what went wrong.
pub async fn call_minimax(config: &ModelsConfig, system_prompt: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(config.analysis_connect_timeout_secs))
        .timeout(Duration::from_secs(config.analysis_request_timeout_secs))
        .build()
        .map_err(|e| format!("failed to build analysis client: {e}"))?;
    let url = build_chat_completions_url(config.effective_analysis_base_url());

    let request = MiniMaxRequest {
        model: config.effective_analysis_model().to_string(),
        messages: vec![MiniMaxMessage {
            role: "user".into(),
            content: system_prompt.into(),
        }],
        temperature: 0.0,
        top_k: 1,
        seed: 42,
        cache_prompt: false,
        reasoning_format: "none".into(),
        response_format: Some(ResponseFormat {
            format_type: "json_object".into(),
        }),
    };

    let mut last_err = String::new();
    for attempt in 0..=MINIMAX_MAX_RETRIES {
        if attempt > 0 {
            let delay = std::time::Duration::from_secs(2u64.pow(attempt));
            tokio::time::sleep(delay).await;
        }

        let resp = match client.post(&url).json(&request).send().await {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("local analyser request failed: {e}");
                continue;
            }
        };

        let status = resp.status();
        if status.as_u16() == 429 || status.as_u16() == 503 || status.as_u16() == 529 {
            let body = resp.text().await.unwrap_or_default();
            last_err = format!("local analyser overloaded HTTP {status}: {body}");
            tracing::warn!(attempt, status = %status, "local analyser overloaded, retrying");
            continue;
        }

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("local analyser returned HTTP {status}: {body}"));
        }

        let parsed: MiniMaxResponse = resp
            .json()
            .await
            .map_err(|e| format!("local analyser response parse failed: {e}"))?;

        let cleaned = parsed
            .choices
            .first()
            .map(|c| clean_model_output(&c.message.content))
            .ok_or_else(|| "local analyser returned empty choices".to_string())?;
        let candidate = extract_json_object(&cleaned)
            .or_else(|| extract_json_object(parsed.choices[0].message.content.as_str()))
            .unwrap_or(cleaned);
        if serde_json::from_str::<serde_json::Value>(&candidate).is_err() {
            last_err = "local analyser did not return valid JSON".into();
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

/// Clean raw MiniMax M2.7 output to extract JSON content.
///
/// Handles two quirks:
/// 1. `<think>…</think>` reasoning tags wrapped around the real content
/// 2. Markdown code fences (` ```json … ``` `) around JSON
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

#[cfg(test)]
mod tests {
    use super::{call_minimax, extract_json_object};
    use crate::config::ModelsConfig;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn call_minimax_uses_local_endpoint_and_parses_json() {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "choices": [
                {
                    "message": {
                        "content": "{\"valid\":true,\"reason\":\"ok\"}"
                    }
                }
            ]
        });
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;

        let cfg = ModelsConfig {
            minimax_api_key: "unused".into(),
            minimax_model: "unused".into(),
            minimax_base_url: "http://unused.invalid".into(),
            opus_api_key: "".into(),
            opus_model: "".into(),
            analysis_base_url: server.uri(),
            analysis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
            analysis_connect_timeout_secs: 5,
            analysis_request_timeout_secs: 10,
            analysis_max_concurrency: 2,
            final_synthesis_base_url: "http://localhost:8087".into(),
            final_synthesis_model: "Qwen3.5-122B-A10B-UD-Q5_K_XL".into(),
            final_synthesis_connect_timeout_secs: 10,
            final_synthesis_request_timeout_secs: 900,
            final_synthesis_warmup_enabled: true,
            final_synthesis_warmup_max_attempts: 0,
            final_synthesis_warmup_delay_secs: 5,
            local_synthesis_base_url: server.uri(),
            local_synthesis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
        };

        let output = call_minimax(&cfg, "Return JSON").await;
        assert_eq!(
            output.unwrap_or_default(),
            "{\"valid\":true,\"reason\":\"ok\"}"
        );
    }

    #[test]
    fn extract_json_object_handles_channel_wrapped_output() {
        let raw = "<|channel>thought\n<channel|>```json\n{\"valid\":true,\"reason\":\"ok\"}\n```";
        let extracted = extract_json_object(raw).expect("should extract json");
        assert_eq!(extracted, "{\"valid\":true,\"reason\":\"ok\"}");
    }
}
