//! Smoke-test response validation, lenient edition (bot-lifecycle spec
//! Part 2): a probe passes when the ingest ladder recovers any non-empty
//! prose from the body. Shape pickiness is gone — the wizard's live checks
//! and the approval smoke share this single rule.

use crate::bot_client::ingest;

/// Validate one smoke-probe response body. `_round` is kept on the
/// signature for call-site compatibility (round-specific structural checks
/// were retired by the unified bot contract, 2026-04-23).
pub(crate) fn validate_smoke_json_for_round(
    json: &serde_json::Value,
    _round: i64,
) -> Result<(), String> {
    let bytes = serde_json::to_vec(json).unwrap_or_default();
    let ingested = ingest::ingest_prose(&bytes);
    if ingested.text.trim().is_empty() {
        return Err(
            "bot returned no readable prose — reply with a JSON body like {\"text\": \"your answer\"} (see /bots/guide)"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_shape_passes() {
        assert!(
            validate_smoke_json_for_round(&serde_json::json!({"text": "an answer"}), 0).is_ok()
        );
    }

    #[test]
    fn response_shape_passes() {
        assert!(
            validate_smoke_json_for_round(&serde_json::json!({"response": "an answer"}), 2).is_ok()
        );
    }

    #[test]
    fn undocumented_field_is_salvaged_and_passes() {
        assert!(
            validate_smoke_json_for_round(&serde_json::json!({"output": "an answer"}), 0).is_ok()
        );
    }

    #[test]
    fn arbitrary_prose_bearing_json_passes() {
        assert!(
            validate_smoke_json_for_round(
                &serde_json::json!({"verdict": "the argument fails on causation"}),
                0
            )
            .is_ok()
        );
    }

    #[test]
    fn proseless_body_fails_with_guidance() {
        let err = validate_smoke_json_for_round(&serde_json::json!({"n": 1}), 0).unwrap_err();
        assert!(err.contains("/bots/guide"));
    }

    #[test]
    fn empty_text_fails() {
        assert!(validate_smoke_json_for_round(&serde_json::json!({"text": "  "}), 0).is_err());
    }
}
