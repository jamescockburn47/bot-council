//! Classify raw bot-call errors into the closed-set taxonomy that
//! `/bots/{id}/history` and Clint's `lqc_bot_diagnose` tool aggregate on.
//!
//! Keys must match the values documented in migration
//! `20260419000001_responses_error_detail.sql`. The classifier is a pure
//! function of the error message + whether the tokio timeout tripped.

/// Classification of a failed bot call. Serialised as a single row in
/// `responses` with `abstained = true` and `valid = false`.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorClassification {
    /// Closed-set taxonomy key (see migration comment).
    pub kind: &'static str,
    /// Short human-readable detail (missing field name, HTTP status, etc.).
    pub detail: String,
}

/// Classify a timeout (tokio::time::timeout elapsed).
pub fn from_timeout(timeout_secs: u64) -> ErrorClassification {
    ErrorClassification {
        kind: "timeout",
        detail: format!("exceeded {timeout_secs}s"),
    }
}

/// Classify a raw error string from `bot_client::send_debate_request`.
///
/// The classifier scans for well-known substrings produced by reqwest and
/// the handler's own `format!()` paths. Unknown strings get `internal`.
pub fn from_client_error(raw: &str) -> ErrorClassification {
    let lower = raw.to_lowercase();

    if lower.contains("connection refused") {
        return ErrorClassification {
            kind: "connection_refused",
            detail: trim_detail(raw),
        };
    }
    if lower.contains("dns error") || lower.contains("failed to lookup") || lower.contains("name resolution") {
        return ErrorClassification {
            kind: "dns",
            detail: trim_detail(raw),
        };
    }
    if lower.contains("tls") || lower.contains("ssl") || lower.contains("certificate") {
        return ErrorClassification {
            kind: "tls",
            detail: trim_detail(raw),
        };
    }
    if let Some(status) = extract_http_status(&lower) {
        if (500..600).contains(&status) {
            return ErrorClassification {
                kind: "http_5xx",
                detail: format!("HTTP {status}"),
            };
        }
        // Credential rejection is its own kind — a 401/403 used to fall
        // into the generic 4xx (and, worse, historic rows show it as
        // schema_missing_field), hiding the actual fix from the owner.
        if status == 401 || status == 403 {
            return ErrorClassification {
                kind: "auth",
                detail: format!("HTTP {status}"),
            };
        }
        if (400..500).contains(&status) {
            return ErrorClassification {
                kind: "http_4xx",
                detail: format!("HTTP {status}"),
            };
        }
    }
    // Schema-specific errors must come BEFORE the generic json_parse
    // branch; "invalid response body: missing field `x`" contains both
    // patterns and we want the more specific one to win.
    if lower.contains("response body too large") {
        return ErrorClassification {
            kind: "schema_invalid_value",
            detail: trim_detail(raw),
        };
    }
    if lower.contains("missing field") {
        let name = lower
            .split("missing field")
            .nth(1)
            .and_then(|s| s.split('`').nth(1))
            .unwrap_or("response")
            .to_string();
        return ErrorClassification {
            kind: "schema_missing_field",
            detail: name,
        };
    }
    if lower.contains("invalid type") {
        return ErrorClassification {
            kind: "schema_invalid_type",
            detail: trim_detail(raw),
        };
    }
    if lower.contains("invalid response body") || lower.contains("not valid json") || lower.contains("expected value") {
        return ErrorClassification {
            kind: "json_parse",
            detail: trim_detail(raw),
        };
    }

    ErrorClassification {
        kind: "internal",
        detail: trim_detail(raw),
    }
}

/// Classify a schema-level validation failure discovered by the analyser
/// (e.g. confidence out of 0–100, malformed challenge JSON). The caller
/// passes `field` = which field failed, and `detail` = human reason.
pub fn from_schema_failure(field: &str, detail: &str) -> ErrorClassification {
    ErrorClassification {
        kind: "schema_invalid_value",
        detail: format!("{field}: {detail}"),
    }
}

/// Truncate a detail string to 200 chars so DB rows stay compact.
/// Uses ASCII `...` (3 bytes) as the truncation marker so byte-length
/// assertions stay exact.
fn trim_detail(raw: &str) -> String {
    const MAX: usize = 200;
    if raw.len() <= MAX {
        raw.to_string()
    } else {
        // Walk to the last char boundary at-or-before MAX to avoid
        // slicing mid-multibyte.
        let mut cut = MAX;
        while !raw.is_char_boundary(cut) && cut > 0 {
            cut -= 1;
        }
        format!("{}...", &raw[..cut])
    }
}

/// Extract a bare HTTP status code from a lowercased error string, if any.
fn extract_http_status(lower: &str) -> Option<u16> {
    let marker = "http ";
    let idx = lower.find(marker)?;
    let tail = &lower[idx + marker.len()..];
    let code: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    if code.len() != 3 {
        return None;
    }
    code.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_timeout() {
        let c = from_timeout(300);
        assert_eq!(c.kind, "timeout");
        assert!(c.detail.contains("300"));
    }

    #[test]
    fn classifies_connection_refused() {
        let c = from_client_error("connection failed: connection refused");
        assert_eq!(c.kind, "connection_refused");
    }

    #[test]
    fn classifies_dns() {
        let c = from_client_error("connection failed: dns error: failed to lookup address information");
        assert_eq!(c.kind, "dns");
    }

    #[test]
    fn classifies_tls() {
        let c = from_client_error("connection failed: error trying to connect: tls handshake eof");
        assert_eq!(c.kind, "tls");
    }

    #[test]
    fn classifies_http_500() {
        let c = from_client_error("bot returned HTTP 500 Internal Server Error");
        assert_eq!(c.kind, "http_5xx");
        assert_eq!(c.detail, "HTTP 500");
    }

    #[test]
    fn classifies_http_401_as_auth() {
        let c = from_client_error("bot returned HTTP 401 Unauthorized");
        assert_eq!(c.kind, "auth");
        assert_eq!(c.detail, "HTTP 401");
    }

    #[test]
    fn classifies_http_403_as_auth() {
        let c = from_client_error("bot returned HTTP 403 Forbidden");
        assert_eq!(c.kind, "auth");
    }

    #[test]
    fn classifies_http_404_as_generic_4xx() {
        let c = from_client_error("bot returned HTTP 404 Not Found");
        assert_eq!(c.kind, "http_4xx");
        assert_eq!(c.detail, "HTTP 404");
    }

    #[test]
    fn classifies_missing_field() {
        let c = from_client_error("invalid response body: missing field `response` at line 1 column 10");
        assert_eq!(c.kind, "schema_missing_field");
        assert_eq!(c.detail, "response");
    }

    #[test]
    fn classifies_json_parse() {
        let c = from_client_error("invalid response body: expected value at line 1 column 1");
        assert_eq!(c.kind, "json_parse");
    }

    #[test]
    fn classifies_oversize_body() {
        let c = from_client_error("response body too large: 20000000 bytes (limit 524288)");
        assert_eq!(c.kind, "schema_invalid_value");
    }

    #[test]
    fn classifies_invalid_type() {
        let c = from_client_error("invalid response body: invalid type: integer `5`, expected a string at line 1 column 15");
        assert_eq!(c.kind, "schema_invalid_type");
    }

    #[test]
    fn unknown_falls_through_to_internal() {
        let c = from_client_error("something wild");
        assert_eq!(c.kind, "internal");
        assert_eq!(c.detail, "something wild");
    }

    #[test]
    fn truncates_long_detail() {
        let long = "x".repeat(500);
        let c = from_client_error(&long);
        assert_eq!(c.kind, "internal");
        // 200 leading chars plus "..." truncation marker.
        assert_eq!(c.detail.len(), 203);
        assert!(c.detail.ends_with("..."));
    }
}
