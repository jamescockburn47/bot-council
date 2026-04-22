//! Structured-field extraction from text-only bot responses.
//!
//! Pipeline: assemble a constrained prompt → call MiniMax → parse response
//! → verify each extracted field's source quote is a verbatim substring of
//! the bot's raw text. Fields whose quotes fail verification are dropped.
//! Fields whose quotes verify are passed through existing round-specific
//! schema validation in `api::bots`.

use crate::config::ModelsConfig;
use serde_json::{Value, json};

pub mod prompt;
pub mod schema;
pub mod verify;

pub use prompt::ExtractTarget;

/// Result of an extraction attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionOutcome {
    /// MiniMax returned a well-formed extraction and every field's quote
    /// was verified as a substring of the bot's raw text. The JSON value
    /// is the fully-validated structured field (shape depends on target).
    Extracted { value: Value, source_quote: String },
    /// MiniMax or the verifier could not confirm the structure is present.
    /// The round continues with the field empty.
    Absent,
    /// A hard error occurred — MiniMax unreachable, unparseable response,
    /// or every quote failed verification. Caller logs and treats as Absent.
    Failed { reason: String },
}

/// Extract a structured field from the bot's raw text.
pub async fn extract_structured_field(
    models: &ModelsConfig,
    target: ExtractTarget,
    bot_text: &str,
) -> ExtractionOutcome {
    if bot_text.trim().is_empty() {
        return ExtractionOutcome::Absent;
    }
    let prompt = prompt::build_extraction_prompt(target, bot_text);
    let raw = match crate::analyser::call_minimax(models, &prompt).await {
        Ok(s) => s,
        Err(e) => {
            return ExtractionOutcome::Failed {
                reason: format!("minimax call failed: {e}"),
            };
        }
    };
    let parsed: schema::RawExtraction = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            return ExtractionOutcome::Failed {
                reason: format!("extractor response not JSON: {e}"),
            };
        }
    };
    if !parsed.extracted {
        return ExtractionOutcome::Absent;
    }
    if parsed.fields.is_empty() {
        return ExtractionOutcome::Absent;
    }
    // Verify every field's quote is a substring of the bot's raw text.
    // Pick one representative quote for the outcome — the longest — so
    // the transcript UI has something meaningful to show. All quotes
    // must verify; if any fail, treat as Absent (not Failed — the model
    // said the structure was present but couldn't back it up).
    let mut representative_quote: Option<String> = None;
    let mut value_map = serde_json::Map::new();
    for (name, field) in parsed.fields.iter() {
        if !verify::quote_is_substring_of(&field.quote, bot_text) {
            return ExtractionOutcome::Absent;
        }
        if representative_quote
            .as_ref()
            .map_or(true, |cur| field.quote.len() > cur.len())
        {
            representative_quote = Some(field.quote.clone());
        }
        value_map.insert(name.clone(), field.value.clone());
    }
    // Reshape into the structured-field JSON expected by existing
    // validate_smoke_json_for_round (challenge/position_change objects).
    let structured = match target {
        ExtractTarget::Challenge => {
            // Must present as {"challenge": {claim_targeted, counter_evidence, type}}
            json!({ "challenge": Value::Object(value_map) })
        }
        ExtractTarget::PositionChange => {
            json!({ "position_change": Value::Object(value_map) })
        }
    };
    let quote = representative_quote.unwrap_or_default();
    ExtractionOutcome::Extracted {
        value: structured,
        source_quote: quote,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ModelsConfig;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Build a ModelsConfig whose analysis endpoint is the mock server.
    /// Mirrors the pattern used in `src/analyser/mod.rs` tests — every
    /// field is set explicitly because `ModelsConfig` does not implement
    /// `Default`. The only field `call_minimax` actually reads is
    /// `analysis_base_url` (via `effective_analysis_base_url`); the rest
    /// must still be valid strings so the struct is constructable.
    fn test_models_config(base_url: &str) -> ModelsConfig {
        ModelsConfig {
            minimax_api_key: "unused".into(),
            minimax_model: "unused".into(),
            minimax_base_url: "http://unused.invalid".into(),
            opus_api_key: "".into(),
            opus_model: "".into(),
            analysis_base_url: base_url.to_string(),
            analysis_model: "test-model".into(),
            analysis_connect_timeout_secs: 2,
            analysis_request_timeout_secs: 10,
            analysis_max_concurrency: 2,
            final_synthesis_base_url: "http://unused.invalid".into(),
            final_synthesis_model: "unused".into(),
            final_synthesis_connect_timeout_secs: 10,
            final_synthesis_request_timeout_secs: 60,
            final_synthesis_warmup_enabled: false,
            final_synthesis_warmup_max_attempts: 0,
            final_synthesis_warmup_delay_secs: 5,
            local_synthesis_base_url: "http://unused.invalid".into(),
            local_synthesis_model: "unused".into(),
        }
    }

    async fn mock_minimax(server: &MockServer, minimax_content: &str) {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": minimax_content}}]
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn extraction_verifies_quote_and_returns_extracted() {
        let server = MockServer::start().await;
        let bot_text = "I challenge the claim that X because evidence Y contradicts it; this is a factual dispute.";
        mock_minimax(&server, r#"{"extracted": true, "fields": {
            "claim_targeted": {"value": "X", "quote": "the claim that X"},
            "counter_evidence": {"value": "evidence Y contradicts it", "quote": "evidence Y contradicts it"},
            "type": {"value": "factual", "quote": "factual dispute"}
        }}"#).await;
        let models = test_models_config(&server.uri());
        let out = extract_structured_field(&models, ExtractTarget::Challenge, bot_text).await;
        match out {
            ExtractionOutcome::Extracted { value, .. } => {
                assert_eq!(value["challenge"]["type"], "factual");
            }
            other => panic!("expected Extracted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn fabricated_quote_is_rejected_as_absent() {
        let server = MockServer::start().await;
        let bot_text = "A harmless, non-challenging sentence.";
        // MiniMax claims extraction succeeded but the quote is not in the text.
        mock_minimax(
            &server,
            r#"{"extracted": true, "fields": {
            "claim_targeted": {"value": "X", "quote": "this quote does not appear"},
            "counter_evidence": {"value": "Y", "quote": "neither does this"},
            "type": {"value": "factual", "quote": "nor this"}
        }}"#,
        )
        .await;
        let models = test_models_config(&server.uri());
        let out = extract_structured_field(&models, ExtractTarget::Challenge, bot_text).await;
        assert_eq!(out, ExtractionOutcome::Absent);
    }

    #[tokio::test]
    async fn model_says_absent_returns_absent() {
        let server = MockServer::start().await;
        mock_minimax(&server, r#"{"extracted": false}"#).await;
        let models = test_models_config(&server.uri());
        let out = extract_structured_field(&models, ExtractTarget::Challenge, "text").await;
        assert_eq!(out, ExtractionOutcome::Absent);
    }

    #[tokio::test]
    async fn unparseable_response_returns_failed() {
        let server = MockServer::start().await;
        mock_minimax(&server, r#"{"unexpected": "object"}"#).await;
        let models = test_models_config(&server.uri());
        let out = extract_structured_field(&models, ExtractTarget::Challenge, "text").await;
        assert!(matches!(out, ExtractionOutcome::Failed { .. }));
    }

    #[tokio::test]
    async fn extracted_true_with_no_fields_is_absent() {
        let server = MockServer::start().await;
        mock_minimax(&server, r#"{"extracted": true}"#).await;
        let models = test_models_config(&server.uri());
        let out = extract_structured_field(&models, ExtractTarget::Challenge, "any text").await;
        assert_eq!(out, ExtractionOutcome::Absent);
    }
}
