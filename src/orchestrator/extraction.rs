//! Post-round structured-field extraction for text_only bots.
//!
//! Called by round 2 and round 4 handlers after bot responses are collected
//! but before they're persisted. For each response from a text_only bot
//! whose required structured field is missing, invoke the extractor and
//! patch the response. Extraction metadata (source + quote) is returned
//! alongside so the caller can persist it into `responses.extraction_metadata`.

use crate::bot_client::DebateRoundResponse;
use crate::config::ModelsConfig;
use crate::extractor::{self, ExtractTarget, ExtractionOutcome};
use serde_json::json;

/// Per-field extraction provenance, serialised as the value of
/// `responses.extraction_metadata`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProvenance {
    pub field: &'static str,  // "challenge" or "position_change"
    pub source: &'static str, // "authored" | "extracted" | "extraction_failed"
    pub quote: Option<String>,
}

impl FieldProvenance {
    /// Serialise this provenance record as the JSON payload persisted into
    /// `responses.extraction_metadata`. Only `source` and `quote` are
    /// written; the field name is conveyed by the surrounding context.
    pub fn to_json(&self) -> serde_json::Value {
        json!({ "source": self.source, "quote": self.quote })
    }
}

/// If the bot is text_only and `response` is missing the structured field
/// required for `target`, run extraction and patch `response` in place.
/// Returns the provenance record to be persisted.
pub async fn extract_if_needed(
    models: &ModelsConfig,
    bot_kind: &str,
    target: ExtractTarget,
    response: &mut DebateRoundResponse,
) -> FieldProvenance {
    let field_name = match target {
        ExtractTarget::Challenge => "challenge",
        ExtractTarget::PositionChange => "position_change",
    };
    if bot_kind != "text_only" {
        return FieldProvenance {
            field: field_name,
            source: "authored",
            quote: None,
        };
    }
    let already_present = match target {
        ExtractTarget::Challenge => response.challenge.is_some(),
        ExtractTarget::PositionChange => response.position_change.is_some(),
    };
    if already_present {
        return FieldProvenance {
            field: field_name,
            source: "authored",
            quote: None,
        };
    }
    let outcome = extractor::extract_structured_field(models, target, &response.response).await;
    match outcome {
        ExtractionOutcome::Extracted {
            value,
            source_quote,
        } => {
            let patched = match target {
                ExtractTarget::Challenge => match serde_json::from_value::<
                    crate::bot_client::ChallengeField,
                >(value["challenge"].clone())
                {
                    Ok(ch) => {
                        response.challenge = Some(ch);
                        true
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "extractor returned malformed challenge shape; provenance downgraded to extraction_failed"
                        );
                        false
                    }
                },
                ExtractTarget::PositionChange => {
                    match serde_json::from_value::<crate::bot_client::PositionChangeField>(
                        value["position_change"].clone(),
                    ) {
                        Ok(pc) => {
                            response.position_change = Some(pc);
                            true
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                "extractor returned malformed position_change shape; provenance downgraded to extraction_failed"
                            );
                            false
                        }
                    }
                }
            };
            if patched {
                FieldProvenance {
                    field: field_name,
                    source: "extracted",
                    quote: Some(source_quote),
                }
            } else {
                FieldProvenance {
                    field: field_name,
                    source: "extraction_failed",
                    quote: None,
                }
            }
        }
        ExtractionOutcome::Absent => FieldProvenance {
            field: field_name,
            source: "extraction_failed",
            quote: None,
        },
        ExtractionOutcome::Failed { .. } => FieldProvenance {
            field: field_name,
            source: "extraction_failed",
            quote: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_client::{ChallengeField, DebateRoundResponse};

    fn empty_resp(text: &str) -> DebateRoundResponse {
        DebateRoundResponse {
            response: text.into(),
            confidence: None,
            challenge: None,
            position_change: None,
        }
    }

    // These two tests short-circuit before any MiniMax call — fast.

    #[tokio::test]
    async fn external_bot_is_never_extracted() {
        let models = test_models_config("http://localhost:0");
        let mut r = empty_resp("some prose");
        let p = extract_if_needed(&models, "external", ExtractTarget::Challenge, &mut r).await;
        assert_eq!(p.source, "authored");
        assert!(r.challenge.is_none());
    }

    #[tokio::test]
    async fn text_only_bot_with_existing_field_is_not_extracted() {
        let models = test_models_config("http://localhost:0");
        let mut r = empty_resp("some prose");
        r.challenge = Some(ChallengeField {
            claim_targeted: "X".into(),
            counter_evidence: "Y".into(),
            challenge_type: "factual".into(),
        });
        let p = extract_if_needed(&models, "text_only", ExtractTarget::Challenge, &mut r).await;
        assert_eq!(p.source, "authored");
    }

    // End-to-end extractor coverage also lives in `tests/text_only_bot_flow.rs`.

    #[tokio::test]
    async fn malformed_extracted_shape_is_downgraded_to_extraction_failed() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // claim_targeted is a number, not a string — passes the extractor's schema
        // (value: serde_json::Value, quote: String) and passes quote verification,
        // but fails deserialisation into the typed ChallengeField.
        let minimax_body = r#"{
            "extracted": true,
            "fields": {
                "claim_targeted": {"value": 123, "quote": "the claim X"},
                "counter_evidence": {"value": "Y", "quote": "evidence Y"},
                "type": {"value": "factual", "quote": "factual dispute"}
            }
        }"#;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{"message": {"content": minimax_body}}]
            })))
            .mount(&server)
            .await;

        let models = test_models_config(&server.uri());
        let mut response =
            empty_resp("I challenge the claim X because of evidence Y; this is a factual dispute.");
        let p = extract_if_needed(
            &models,
            "text_only",
            ExtractTarget::Challenge,
            &mut response,
        )
        .await;

        assert_eq!(
            p.source, "extraction_failed",
            "must not lie — serde failure is not 'extracted'"
        );
        assert!(p.quote.is_none());
        assert!(
            response.challenge.is_none(),
            "response must remain unpatched on serde failure"
        );
    }

    /// Minimal ModelsConfig for tests. Every field set explicitly because
    /// `ModelsConfig` has no `Default` impl. Mirrors the pattern in
    /// `src/extractor/mod.rs` tests — `analysis_base_url` is the only field
    /// that matters (it routes `call_minimax` at the mock server); the rest
    /// must still be valid strings so the struct is constructable. Pass
    /// `"http://localhost:0"` (or any dummy) for tests that short-circuit
    /// before a network call.
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
}
