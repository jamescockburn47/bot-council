//! Serde types for the JSON shape MiniMax is instructed to return.
//! Deliberately tolerant — upstream validation happens after quote
//! verification, so deserialisation only needs to succeed for well-formed
//! outputs and fail cleanly for everything else.

use serde::Deserialize;
use std::collections::BTreeMap;

/// Top-level extractor response.
#[derive(Debug, Deserialize)]
pub struct RawExtraction {
    pub extracted: bool,
    #[serde(default)]
    pub fields: BTreeMap<String, RawField>,
}

/// One extracted field: a value plus the source quote that supports it.
#[derive(Debug, Deserialize)]
pub struct RawField {
    pub value: serde_json::Value,
    pub quote: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_absent_extraction() {
        let r: RawExtraction = serde_json::from_str(r#"{"extracted": false}"#).unwrap();
        assert!(!r.extracted);
        assert!(r.fields.is_empty());
    }

    #[test]
    fn parses_challenge_extraction() {
        let json = r#"{
            "extracted": true,
            "fields": {
                "claim_targeted": {"value": "X", "quote": "claims X"},
                "counter_evidence": {"value": "Y", "quote": "but Y"},
                "type": {"value": "factual", "quote": "on the facts"}
            }
        }"#;
        let r: RawExtraction = serde_json::from_str(json).unwrap();
        assert!(r.extracted);
        assert_eq!(r.fields.len(), 3);
        assert_eq!(r.fields["type"].value, serde_json::json!("factual"));
    }

    #[test]
    fn missing_fields_key_is_tolerated() {
        let r: RawExtraction = serde_json::from_str(r#"{"extracted": true}"#).unwrap();
        assert!(r.extracted);
        assert!(r.fields.is_empty());
    }
}
