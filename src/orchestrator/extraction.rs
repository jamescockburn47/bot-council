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
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Per-field extraction provenance, serialised as the value of
/// `responses.extraction_metadata`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldProvenance {
    pub field: &'static str, // "challenge" | "position_change" | "crux_engagement" | "steelman"
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

/// Engagement stance a bot can take on the R3 crux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CruxEngagementStance {
    /// Bot agrees with the crux claim.
    Agreed,
    /// Bot partially rejects the crux claim (concedes part, contests part).
    PartiallyRejected,
    /// Bot rejects the crux claim.
    Rejected,
    /// Bot rejects the framing of the crux itself (frame dispute).
    FrameRejected,
}

/// Run the extraction sentinels over a finished provenance record; log any
/// violations and return the record unchanged (sentinels never block —
/// they make breakage loud, see src/observability/sentinels.rs).
fn checked(provenance: FieldProvenance, raw_response: &str) -> FieldProvenance {
    crate::observability::sentinels::log_violations(
        "extraction",
        &crate::observability::sentinels::check_provenance(&provenance, raw_response),
    );
    provenance
}

/// Result of the R3 crux-engagement extraction. Returned alongside the
/// `FieldProvenance` so callers can persist both the provenance and the
/// classified stance (when available) into `responses.extraction_metadata`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CruxEngagementExtraction {
    pub stance: Option<CruxEngagementStance>,
    pub provenance: FieldProvenance,
}

impl CruxEngagementExtraction {
    /// Serialise as the value stored under `extraction_metadata.crux_engagement`.
    /// Shape: `{source, quote, stance?}` — stance is only present when the
    /// extraction succeeded and the quote verified.
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        obj.insert("source".into(), json!(self.provenance.source));
        obj.insert("quote".into(), json!(self.provenance.quote));
        if let Some(stance) = self.stance {
            // Round-tripping through serde_json gives snake_case string.
            if let Ok(v) = serde_json::to_value(stance) {
                obj.insert("stance".into(), v);
            }
        }
        serde_json::Value::Object(obj)
    }
}

/// Raw MiniMax output shape for the crux-engagement extractor.
#[derive(Debug, Deserialize)]
struct RawCruxEngagement {
    engagement_stance: CruxEngagementStance,
    reasoning_quote: String,
}

/// Extract a bot's crux-engagement stance from its R3 prose.
///
/// MiniMax returns `{"engagement_stance": <stance>, "reasoning_quote": <substring>}`.
/// If the reasoning quote is not a verbatim substring of the bot's text, the
/// result is downgraded to `source: "extraction_failed"` — matching the
/// existing challenge/position_change quote-verification policy.
///
/// Runs against every bot's R3 prose regardless of `bot_kind` — the external
/// vs text-only distinction no longer gates extraction. MiniMax errors,
/// malformed JSON, or quote-verify failure all downgrade to
/// `extraction_failed` — never propagate an error upward.
///
/// `_bot_kind` is kept on the signature for call-site compatibility during
/// the transition off of dual-contract bots; callers should pass it through.
pub async fn extract_crux_engagement(
    models: &ModelsConfig,
    _bot_kind: &str,
    bot_text: &str,
) -> CruxEngagementExtraction {
    let field_name = "crux_engagement";
    if bot_text.trim().is_empty() {
        return CruxEngagementExtraction {
            stance: None,
            provenance: FieldProvenance {
                field: field_name,
                source: "extraction_failed",
                quote: None,
            },
        };
    }
    let prompt = format!(
        "Classify the bot's engagement with the R3 crux. Return exactly:\n\
         {{\"engagement_stance\": \"<agreed|partially_rejected|rejected|frame_rejected>\",\n\
          \"reasoning_quote\": \"<verbatim substring from the bot's text supporting the stance>\"}}\n\
         \n\
         Bot text:\n{bot_text}"
    );
    let raw = match crate::analyser::call_minimax(models, &prompt).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "crux_engagement extraction MiniMax call failed; provenance downgraded"
            );
            return CruxEngagementExtraction {
                stance: None,
                provenance: FieldProvenance {
                    field: field_name,
                    source: "extraction_failed",
                    quote: None,
                },
            };
        }
    };
    let parsed: RawCruxEngagement = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "crux_engagement extractor returned malformed JSON; provenance downgraded"
            );
            return CruxEngagementExtraction {
                stance: None,
                provenance: FieldProvenance {
                    field: field_name,
                    source: "extraction_failed",
                    quote: None,
                },
            };
        }
    };
    if !extractor::verify::quote_is_substring_of(&parsed.reasoning_quote, bot_text) {
        tracing::warn!(
            "crux_engagement extractor quote not a verbatim substring; provenance downgraded"
        );
        return CruxEngagementExtraction {
            stance: None,
            provenance: FieldProvenance {
                field: field_name,
                source: "extraction_failed",
                quote: None,
            },
        };
    }
    CruxEngagementExtraction {
        stance: Some(parsed.engagement_stance),
        provenance: checked(
            FieldProvenance {
                field: field_name,
                source: "extracted",
                quote: Some(parsed.reasoning_quote),
            },
            bot_text,
        ),
    }
}

/// Result of the R4 steelman extraction. Returned alongside the
/// `FieldProvenance` so callers can persist both the provenance and the
/// extracted steelman text into `responses.extraction_metadata`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteelmanExtraction {
    pub steelman: Option<String>,
    pub provenance: FieldProvenance,
}

impl SteelmanExtraction {
    /// Serialise as the value stored under `extraction_metadata.steelman`.
    /// Shape: `{source, quote, steelman?}` — steelman text is only present
    /// when the extraction succeeded and the quote verified.
    pub fn to_json(&self) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        obj.insert("source".into(), json!(self.provenance.source));
        obj.insert("quote".into(), json!(self.provenance.quote));
        if let Some(s) = &self.steelman {
            obj.insert("steelman".into(), json!(s));
        }
        serde_json::Value::Object(obj)
    }
}

/// Raw MiniMax output shape for the steelman extractor.
#[derive(Debug, Deserialize)]
struct RawSteelman {
    steelman: String,
    source_quote: String,
}

/// Extract the steelman a bot gave in its R4 prose.
///
/// MiniMax returns `{"steelman": <string>, "source_quote": <verbatim substring>}`.
/// The source quote must be a verbatim substring of the bot's text; on
/// failure the result downgrades to `source: "extraction_failed"`, matching
/// the existing challenge/position_change quote-verification policy.
///
/// Runs against every bot's R4 prose regardless of `bot_kind`. MiniMax
/// errors, malformed JSON, or quote-verify failure all downgrade to
/// `extraction_failed` — never propagate an error upward.
pub async fn extract_steelman(
    models: &ModelsConfig,
    _bot_kind: &str,
    bot_text: &str,
) -> SteelmanExtraction {
    let field_name = "steelman";
    if bot_text.trim().is_empty() {
        return SteelmanExtraction {
            steelman: None,
            provenance: FieldProvenance {
                field: field_name,
                source: "extraction_failed",
                quote: None,
            },
        };
    }
    let prompt = format!(
        "Extract the bot's steelman — the strongest version of the opposing argument\n\
         articulated in 2-3 sentences. Return exactly:\n\
         {{\"steelman\": \"<the 2-3 sentence opposing argument articulation>\",\n\
          \"source_quote\": \"<verbatim substring from the bot's text that contains the steelman>\"}}\n\
         \n\
         Bot text:\n{bot_text}"
    );
    let raw = match crate::analyser::call_minimax(models, &prompt).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "steelman extraction MiniMax call failed; provenance downgraded"
            );
            return SteelmanExtraction {
                steelman: None,
                provenance: FieldProvenance {
                    field: field_name,
                    source: "extraction_failed",
                    quote: None,
                },
            };
        }
    };
    let parsed: RawSteelman = match serde_json::from_str(&raw) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "steelman extractor returned malformed JSON; provenance downgraded"
            );
            return SteelmanExtraction {
                steelman: None,
                provenance: FieldProvenance {
                    field: field_name,
                    source: "extraction_failed",
                    quote: None,
                },
            };
        }
    };
    if !extractor::verify::quote_is_substring_of(&parsed.source_quote, bot_text) {
        tracing::warn!("steelman extractor quote not a verbatim substring; provenance downgraded");
        return SteelmanExtraction {
            steelman: None,
            provenance: FieldProvenance {
                field: field_name,
                source: "extraction_failed",
                quote: None,
            },
        };
    }
    SteelmanExtraction {
        steelman: Some(parsed.steelman),
        provenance: checked(
            FieldProvenance {
                field: field_name,
                source: "extracted",
                quote: Some(parsed.source_quote),
            },
            bot_text,
        ),
    }
}

/// If `response` is missing the structured field required for `target`, run
/// extraction and patch `response` in place. Returns the provenance record
/// to be persisted.
///
/// Unified across `bot_kind`: if the bot supplied the field in its response
/// (external-style structured output), that's "authored" — no MiniMax call.
/// Otherwise we extract from prose.
pub async fn extract_if_needed(
    models: &ModelsConfig,
    _bot_kind: &str,
    target: ExtractTarget,
    response: &mut DebateRoundResponse,
) -> FieldProvenance {
    let field_name = match target {
        ExtractTarget::Challenge => "challenge",
        ExtractTarget::PositionChange => "position_change",
    };
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
                checked(
                    FieldProvenance {
                        field: field_name,
                        source: "extracted",
                        quote: Some(source_quote),
                    },
                    &response.response,
                )
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
            ingest_kind: Default::default(),
        }
    }

    // These two tests short-circuit before any MiniMax call — fast.

    #[tokio::test]
    async fn external_bot_with_existing_field_is_not_extracted() {
        // `bot_kind` no longer gates extraction; what matters is whether
        // the response already carries the field. External bots that
        // author structured fields keep the "authored" provenance and
        // avoid the extractor round-trip — same as text_only bots that
        // happen to emit the field.
        let models = test_models_config("http://localhost:0");
        let mut r = empty_resp("some prose");
        r.challenge = Some(ChallengeField {
            claim_targeted: "X".into(),
            counter_evidence: "Y".into(),
            challenge_type: "factual".into(),
        });
        let p = extract_if_needed(&models, "external", ExtractTarget::Challenge, &mut r).await;
        assert_eq!(p.source, "authored");
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
