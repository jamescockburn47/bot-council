//! Owner-facing guidance for every `error_kind` value: one table, three
//! consumers (onboarding wizard live checks, owner monitoring page, admin
//! views — bot-lifecycle spec Part 3). Plain English, no jargon; every
//! description pairs with a concrete fix hint.

/// Owner-facing explanation of a failed dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guidance {
    /// What happened, in plain English.
    pub description: &'static str,
    /// What the bot's owner should do about it.
    pub fix_hint: &'static str,
}

/// Every kind the classifier can emit. Kept in sync with
/// `error_kind::from_client_error` / `from_timeout` / `from_schema_failure`
/// by the exhaustiveness test below.
pub const KNOWN_KINDS: [&str; 12] = [
    "timeout",
    "connection_refused",
    "dns",
    "tls",
    "auth",
    "http_5xx",
    "http_4xx",
    "json_parse",
    "schema_missing_field",
    "schema_invalid_type",
    "schema_invalid_value",
    "internal",
];

/// Look up the guidance for an error kind. Unknown kinds get a safe
/// fallback rather than a panic or an empty string.
#[must_use]
pub fn for_kind(kind: &str) -> Guidance {
    match kind {
        "timeout" => Guidance {
            description: "Your bot did not answer within the round's time budget.",
            fix_hint: "Tighten your bot's internal time budget to around 120 seconds so it always answers before the council's limit.",
        },
        "connection_refused" => Guidance {
            description: "The council reached your server, but nothing was listening on the port.",
            fix_hint: "Check that your bot process is running and listening on the URL you registered.",
        },
        "dns" => Guidance {
            description: "Your endpoint's hostname did not resolve.",
            fix_hint: "Check the URL for typos; if you use a tunnel, check it is still up — free tunnels rotate their hostnames.",
        },
        "tls" => Guidance {
            description: "The secure connection to your endpoint failed.",
            fix_hint: "Your HTTPS certificate is missing, expired, or self-signed. Use a valid certificate (tunnel providers issue these automatically).",
        },
        "auth" => Guidance {
            description: "Your endpoint rejected the council's credentials.",
            fix_hint: "Check that your bot accepts the exact token you registered — the council sends it as 'Authorization: Bearer <token>'.",
        },
        "http_5xx" => Guidance {
            description: "Your bot returned a server error.",
            fix_hint: "Check your bot's own logs for the crash or exception behind the 5xx response.",
        },
        "http_4xx" => Guidance {
            description: "Your bot rejected the council's request.",
            fix_hint: "Check the endpoint path is right and that your bot accepts POST requests with a JSON body.",
        },
        // The three shape kinds below are historical: lenient ingest now
        // salvages anything prose-shaped, so new rows should not carry them.
        "json_parse" => Guidance {
            description: "Your bot's response could not be read (historical — responses are now salvaged leniently).",
            fix_hint: "Return a JSON body like {\"text\": \"your answer\"}.",
        },
        "schema_missing_field" => Guidance {
            description: "Your bot's response had no readable answer field (historical — responses are now salvaged leniently). Older rows with this kind were often a mislabelled token rejection.",
            fix_hint: "Return {\"text\": \"your answer\"} — and if this appeared alongside HTTP 401/403, check your token.",
        },
        "schema_invalid_type" => Guidance {
            description: "Your bot's answer field was not text (historical — responses are now salvaged leniently).",
            fix_hint: "Make sure the answer value is a string.",
        },
        "schema_invalid_value" => Guidance {
            description: "Your bot's response failed a validity check (for example, an out-of-range confidence).",
            fix_hint: "Keep confidence between 0 and 100 and the answer under 20 KB — oversize text is now truncated rather than rejected.",
        },
        "internal" => Guidance {
            description: "The council hit an unexpected error handling your bot's response.",
            fix_hint: "This one is on us — the operator can check the council's logs. Retry usually succeeds.",
        },
        _ => Guidance {
            description: "An unrecognised error was recorded for this dispatch.",
            fix_hint: "The operator can check the council's logs for the underlying cause.",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_kind_has_substantive_guidance() {
        for kind in KNOWN_KINDS {
            let g = for_kind(kind);
            assert!(!g.description.is_empty(), "{kind} description empty");
            assert!(!g.fix_hint.is_empty(), "{kind} fix_hint empty");
            // The fallback text must not leak into known kinds.
            assert_ne!(
                g.description, "An unrecognised error was recorded for this dispatch.",
                "{kind} fell through to the fallback"
            );
        }
    }

    #[test]
    fn auth_guidance_names_the_token() {
        let g = for_kind("auth");
        assert!(g.fix_hint.contains("token"));
        assert!(g.fix_hint.contains("Bearer"));
    }

    #[test]
    fn unknown_kind_gets_fallback() {
        let g = for_kind("brand_new_kind");
        assert!(g.description.contains("unrecognised"));
    }

    #[test]
    fn known_kinds_cover_the_classifier_outputs() {
        // Kinds the classifier module can emit today. If you add an arm to
        // error_kind.rs, add the kind here AND a Guidance entry above.
        for kind in [
            crate::orchestrator::error_kind::from_timeout(1).kind,
            crate::orchestrator::error_kind::from_client_error("connection refused").kind,
            crate::orchestrator::error_kind::from_client_error("dns error").kind,
            crate::orchestrator::error_kind::from_client_error("tls handshake").kind,
            crate::orchestrator::error_kind::from_client_error("bot returned HTTP 401").kind,
            crate::orchestrator::error_kind::from_client_error("bot returned HTTP 404").kind,
            crate::orchestrator::error_kind::from_client_error("bot returned HTTP 500").kind,
            crate::orchestrator::error_kind::from_client_error("invalid response body: expected value").kind,
            crate::orchestrator::error_kind::from_client_error("missing field `x`").kind,
            crate::orchestrator::error_kind::from_client_error("invalid type: integer").kind,
            crate::orchestrator::error_kind::from_schema_failure("confidence", "out of range").kind,
            crate::orchestrator::error_kind::from_client_error("anything else").kind,
        ] {
            assert!(KNOWN_KINDS.contains(&kind), "classifier emits unlisted kind {kind}");
        }
    }
}
