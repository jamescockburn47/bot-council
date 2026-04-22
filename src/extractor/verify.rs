//! Source-quote substring verification.
//!
//! The load-bearing anti-hallucination guardrail: an extracted field is only
//! accepted if its declared source quote is present verbatim in the bot's
//! raw response. Whitespace runs are normalised on both sides before
//! comparison; the match is otherwise case-sensitive and literal.

/// Returns true iff `quote` appears in `haystack` after whitespace normalisation.
///
/// Whitespace normalisation collapses any run of ASCII whitespace
/// (spaces, tabs, newlines, carriage returns) to a single space and
/// trims leading/trailing whitespace on both sides. The comparison is
/// otherwise case-sensitive and literal — no punctuation or unicode
/// normalisation is applied.
pub fn quote_is_substring_of(quote: &str, haystack: &str) -> bool {
    fn normalise(input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut last_was_space = true;
        for ch in input.chars() {
            if ch.is_ascii_whitespace() {
                if !last_was_space {
                    out.push(' ');
                    last_was_space = true;
                }
            } else {
                out.push(ch);
                last_was_space = false;
            }
        }
        out.trim().to_string()
    }
    let needle = normalise(quote);
    if needle.is_empty() {
        return false;
    }
    let hay = normalise(haystack);
    hay.contains(&needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_quote_is_not_a_valid_source() {
        assert!(!quote_is_substring_of("", "anything"));
    }

    #[test]
    fn exact_substring_matches() {
        let text = "The proposal improves reliability by introducing preflight checks.";
        assert!(quote_is_substring_of("improves reliability", text));
    }

    #[test]
    fn whitespace_variants_match() {
        let text = "The  proposal\nimproves\treliability.";
        assert!(quote_is_substring_of("The proposal improves reliability.", text));
    }

    #[test]
    fn case_sensitive_rejects_wrong_case() {
        let text = "The proposal improves reliability.";
        assert!(!quote_is_substring_of("THE PROPOSAL", text));
    }

    #[test]
    fn invented_quote_fails() {
        let text = "The proposal improves reliability.";
        assert!(!quote_is_substring_of("the opposite is true", text));
    }

    #[test]
    fn leading_trailing_whitespace_is_trimmed() {
        let text = "The proposal improves reliability.";
        assert!(quote_is_substring_of("  improves reliability  ", text));
    }
}
