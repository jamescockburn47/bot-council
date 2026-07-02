//! The authored event catalogue: kind → severity + plain-English narrative
//! + suggested action (operator-legibility spec Part 1). One table, every
//! journal consumer renders from it. Narratives are deterministic
//! templates — never LLM-generated (the spec's binding boundary): the
//! journal is a record, and records do not hallucinate.

/// A composed journal entry body, ready to store.
#[derive(Debug, Clone)]
pub struct EventTemplate {
    /// 'info' | 'attention' | 'problem'.
    pub severity: &'static str,
    /// Plain English, complete sentences, no jargon.
    pub narrative: String,
    /// What the operator should do, when there is something to do.
    pub suggested_action: Option<&'static str>,
}

/// Every kind a writer may record in this phase. A closed-set test keeps
/// code and catalogue in sync. Bot-scoped and extraction-level kinds land
/// with lifecycle Phase 4 (no DB handle at those seams yet).
pub const KNOWN_EVENT_KINDS: [&str; 7] = [
    "service_started",
    "model_route_changed",
    "debate_failed",
    "quorum_not_met",
    "synthesis_fallback",
    "sentinel_violation",
    "resynth_run",
];

/// Compose the entry body for an event.
///
/// `scope_label` is a short human handle ("debate 1a2b3c4d", "the analysis
/// route"); `detail` is one plain-English clause the writer supplies (may
/// be empty). Unknown kinds compose a safe fallback — the closed-set test
/// stops them reaching production.
#[must_use]
pub fn compose(kind: &str, scope_label: &str, detail: &str) -> EventTemplate {
    let detail_sentence = if detail.trim().is_empty() {
        String::new()
    } else {
        format!(" {}", detail.trim())
    };
    match kind {
        "service_started" => EventTemplate {
            severity: "info",
            narrative: format!("The council restarted.{detail_sentence}"),
            suggested_action: None,
        },
        "model_route_changed" => EventTemplate {
            severity: "attention",
            narrative: format!(
                "The AI model behind {scope_label} is different from the previous start.{detail_sentence} \
                 If you made this change, no action is needed — this entry exists so route changes are never invisible."
            ),
            suggested_action: Some(
                "If you did not expect this, check /etc/bot-council.env on EVO — see RUNBOOK.md \u{00a7}2 (summariser problems).",
            ),
        },
        "debate_failed" => EventTemplate {
            severity: "problem",
            narrative: format!("{scope_label} could not finish.{detail_sentence}"),
            suggested_action: Some(
                "Open the debate to see how far it got. If this keeps happening, copy this entry for an agent.",
            ),
        },
        "quorum_not_met" => EventTemplate {
            severity: "problem",
            narrative: format!(
                "{scope_label} was cancelled before it began: fewer than three debaters were reachable.{detail_sentence}"
            ),
            suggested_action: Some(
                "Check the bots page for unreachable debaters, then create the debate again.",
            ),
        },
        "synthesis_fallback" => EventTemplate {
            severity: "attention",
            narrative: format!(
                "The summary for {scope_label} couldn't be fully structured, so a simplified version was stored. \
                 The debate itself is intact.{detail_sentence}"
            ),
            suggested_action: Some(
                "Re-running the summariser usually fixes this — RUNBOOK.md \u{00a7}5 (rebuild summaries).",
            ),
        },
        "sentinel_violation" => EventTemplate {
            severity: "attention",
            narrative: format!(
                "A built-in self-check flagged unexpected output in {scope_label}.{detail_sentence} \
                 Nothing was blocked; the flagged item is marked so it can be reviewed."
            ),
            suggested_action: Some(
                "If this recurs, copy this entry for an agent — the self-check ID inside pinpoints the invariant.",
            ),
        },
        "resynth_run" => EventTemplate {
            severity: "info",
            narrative: format!("Summaries were rebuilt.{detail_sentence}"),
            suggested_action: None,
        },
        _ => EventTemplate {
            severity: "attention",
            narrative: format!(
                "Something happened that this version doesn't have words for yet (kind: {kind}, scope: {scope_label}).{detail_sentence}"
            ),
            suggested_action: Some("Copy this entry for an agent to catalogue properly."),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_kind_composes_substantively() {
        for kind in KNOWN_EVENT_KINDS {
            let t = compose(kind, "debate 1a2b3c4d", "extra detail.");
            assert!(!t.narrative.is_empty(), "{kind} narrative empty");
            assert!(
                matches!(t.severity, "info" | "attention" | "problem"),
                "{kind} severity {} outside closed set",
                t.severity
            );
            assert!(
                !t.narrative.contains("doesn't have words for"),
                "{kind} fell through to the fallback"
            );
            if t.severity != "info" {
                assert!(t.suggested_action.is_some(), "{kind} has no action");
            }
        }
    }

    #[test]
    fn unknown_kind_gets_safe_fallback() {
        let t = compose("brand_new_kind", "somewhere", "");
        assert_eq!(t.severity, "attention");
        assert!(t.narrative.contains("brand_new_kind"));
    }

    #[test]
    fn empty_detail_leaves_no_dangling_space() {
        let t = compose("service_started", "", "");
        assert_eq!(t.narrative, "The council restarted.");
    }
}
