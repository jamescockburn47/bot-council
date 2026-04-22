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
    pub field: &'static str, // "challenge" or "position_change"
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
            match target {
                ExtractTarget::Challenge => {
                    if let Ok(ch) = serde_json::from_value(value["challenge"].clone()) {
                        response.challenge = Some(ch);
                    }
                }
                ExtractTarget::PositionChange => {
                    if let Ok(pc) = serde_json::from_value(value["position_change"].clone()) {
                        response.position_change = Some(pc);
                    }
                }
            }
            FieldProvenance {
                field: field_name,
                source: "extracted",
                quote: Some(source_quote),
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
        let models = test_models_config();
        let mut r = empty_resp("some prose");
        let p = extract_if_needed(&models, "external", ExtractTarget::Challenge, &mut r).await;
        assert_eq!(p.source, "authored");
        assert!(r.challenge.is_none());
    }

    #[tokio::test]
    async fn text_only_bot_with_existing_field_is_not_extracted() {
        let models = test_models_config();
        let mut r = empty_resp("some prose");
        r.challenge = Some(ChallengeField {
            claim_targeted: "X".into(),
            counter_evidence: "Y".into(),
            challenge_type: "factual".into(),
        });
        let p = extract_if_needed(&models, "text_only", ExtractTarget::Challenge, &mut r).await;
        assert_eq!(p.source, "authored");
    }

    // The real extractor path is exercised in `tests/text_only_bot_flow.rs` (Task 14)
    // where a wiremock MiniMax server can be stood up.

    /// Minimal ModelsConfig for tests that short-circuit before any MiniMax call.
    /// Every field set explicitly because ModelsConfig has no Default impl.
    /// Mirrors the pattern in `src/extractor/mod.rs` tests. The URL/key
    /// values don't matter because these tests return before any network
    /// call fires, but every field must be set to construct the struct.
    fn test_models_config() -> ModelsConfig {
        ModelsConfig {
            minimax_api_key: "unused".into(),
            minimax_model: "unused".into(),
            minimax_base_url: "http://unused.invalid".into(),
            opus_api_key: "".into(),
            opus_model: "".into(),
            analysis_base_url: "http://unused.invalid".into(),
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
