/// Normalise bot responses before storage.
///
/// Bots sometimes return prose with embedded JSON rather than clean structured
/// output. This module extracts structured fields from messy responses so the
/// council can store and forward clean data.

use crate::bot_client::{ChallengeField, DebateRoundResponse, PositionChangeField};

/// Try to extract a valid JSON object starting with `{"response"` from text
/// that may contain preamble prose. Counts braces to find the matching close.
fn try_extract_json(text: &str) -> Option<String> {
    let needle = r#"{"response""#;
    let mut search_from = 0;
    while search_from < text.len() {
        let start = text[search_from..].find(needle)?;
        let abs_start = search_from + start;

        let mut depth = 0i32;
        let mut in_string = false;
        let mut escape = false;
        let bytes = text.as_bytes();
        for i in abs_start..bytes.len() {
            let ch = bytes[i];
            if escape { escape = false; continue; }
            if ch == b'\\' && in_string { escape = true; continue; }
            if ch == b'"' { in_string = !in_string; continue; }
            if in_string { continue; }
            if ch == b'{' { depth += 1; }
            if ch == b'}' { depth -= 1; }
            if depth == 0 {
                let candidate = &text[abs_start..=i];
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return Some(candidate.to_string());
                }
                break;
            }
        }
        search_from = abs_start + 1;
    }
    None
}

/// Extract a confidence integer from text (e.g. `"confidence": 72`).
fn extract_confidence(text: &str) -> Option<i64> {
    let needle = r#""confidence""#;
    let pos = text.find(needle)?;
    let after = &text[pos + needle.len()..];
    // Skip whitespace and colon
    let after = after.trim_start().strip_prefix(':')?;
    let after = after.trim_start();
    // Parse the integer
    let end = after.find(|c: char| !c.is_ascii_digit() && c != '-')?;
    if end == 0 { return None; }
    after[..end].parse().ok()
}

/// Extract a challenge object from text containing `"challenge":{...}`.
fn extract_challenge(text: &str) -> Option<ChallengeField> {
    let needle = r#""challenge""#;
    let pos = text.find(needle)?;
    let after_key = &text[pos + needle.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    if !after_colon.starts_with('{') { return None; }

    // Find matching brace
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, ch) in after_colon.bytes().enumerate() {
        if escape { escape = false; continue; }
        if ch == b'\\' && in_string { escape = true; continue; }
        if ch == b'"' { in_string = !in_string; continue; }
        if in_string { continue; }
        if ch == b'{' { depth += 1; }
        if ch == b'}' { depth -= 1; }
        if depth == 0 {
            return serde_json::from_str(&after_colon[..=i]).ok();
        }
    }
    None
}

/// Extract a position_change object from text.
fn extract_position_change(text: &str) -> Option<PositionChangeField> {
    let needle = r#""position_change""#;
    let pos = text.find(needle)?;
    let after_key = &text[pos + needle.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    if !after_colon.starts_with('{') { return None; }

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, ch) in after_colon.bytes().enumerate() {
        if escape { escape = false; continue; }
        if ch == b'\\' && in_string { escape = true; continue; }
        if ch == b'"' { in_string = !in_string; continue; }
        if in_string { continue; }
        if ch == b'{' { depth += 1; }
        if ch == b'}' { depth -= 1; }
        if depth == 0 {
            return serde_json::from_str(&after_colon[..=i]).ok();
        }
    }
    None
}

/// Normalise a bot response by extracting structured fields from prose.
///
/// If the response text contains embedded JSON with a `response` field,
/// replaces the entire response with the extracted structured version.
/// Otherwise, attempts to extract individual fields (confidence, challenge,
/// position_change) from the text.
pub fn normalise_response(raw: &mut DebateRoundResponse) {
    // Try full JSON extraction first — handles "preamble then JSON" pattern
    if let Some(json_str) = try_extract_json(&raw.response) {
        if let Ok(parsed) = serde_json::from_str::<DebateRoundResponse>(&json_str) {
            tracing::info!("response_parser: extracted embedded JSON from bot response");
            *raw = parsed;
            return;
        }
    }

    // Extract individual fields if not already present
    if raw.confidence.is_none() {
        if let Some(c) = extract_confidence(&raw.response) {
            tracing::info!(confidence = c, "response_parser: extracted confidence from text");
            raw.confidence = Some(c);
        }
    }

    if raw.challenge.is_none() {
        if let Some(c) = extract_challenge(&raw.response) {
            tracing::info!("response_parser: extracted challenge from text");
            raw.challenge = Some(c);
        }
    }

    if raw.position_change.is_none() {
        if let Some(pc) = extract_position_change(&raw.response) {
            tracing::info!("response_parser: extracted position_change from text");
            raw.position_change = Some(pc);
        }
    }
}
