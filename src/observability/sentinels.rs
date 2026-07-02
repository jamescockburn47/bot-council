//! Sentinel invariants: named runtime invariants with stable IDs.
//!
//! A sentinel is an invariant that must hold on live output. Violations are
//! returned as data and logged — never panics; a violated sentinel means the
//! output shipped anyway with a loud, greppable warning carrying the stable
//! ID. The committed inventory (`docs/sentinels.md`) is regenerated from
//! code by a test, so docs cannot drift from the source of truth.
//!
//! Pattern ported from the compliance-verifyer repo's observability
//! foundation (2026-07-02 hygiene review).

use crate::orchestrator::extraction::FieldProvenance;
use crate::synthesiser::schema::SessionArtifact;

/// A named runtime invariant with a stable, greppable ID.
#[derive(Debug, Clone, Copy)]
pub struct Sentinel {
    /// Stable ID (`EXT-001`, `SYN-002`, …). Never renumber or reuse.
    pub id: &'static str,
    /// One-sentence statement of the invariant.
    pub statement: &'static str,
}

/// A sentinel violation, as data.
#[derive(Debug, Clone)]
pub struct Violation {
    /// The violated sentinel's stable ID.
    pub sentinel_id: &'static str,
    /// What specifically went wrong.
    pub detail: String,
}

impl Violation {
    fn of(sentinel: &Sentinel, detail: String) -> Self {
        Self {
            sentinel_id: sentinel.id,
            detail,
        }
    }
}

/// EXT-001 — provenance `source` values form a closed set. A novel label
/// would silently break the transcript UI's provenance badges.
pub const EXTRACTION_SOURCE_CLOSED_SET: Sentinel = Sentinel {
    id: "EXT-001",
    statement: "Extraction provenance source is one of: authored, extracted, extraction_failed.",
};

/// EXT-002 — the anti-hallucination story (operational lesson 17): a record
/// labelled `extracted` carries a quote that is a verbatim substring of the
/// bot's raw response. A lying `extracted` label breaks the feature's
/// credibility even when the field content happens to be right.
pub const EXTRACTED_IMPLIES_VERIFIED_QUOTE: Sentinel = Sentinel {
    id: "EXT-002",
    statement: "source == extracted implies a quote that verifies as a substring of the raw response.",
};

/// SYN-001 — when crux selection succeeded, the artifact names the crux
/// (spec: 2026-07-02-issue-centric-sessions-design.md Part 1). Exactly one
/// issue carries `is_crux`.
pub const CRUX_REACHES_ARTIFACT: Sentinel = Sentinel {
    id: "SYN-001",
    statement: "A successful crux selection yields exactly one is_crux issue in the artifact.",
};

/// SYN-002 — meta_observations opens with the contract heading the UI and
/// downstream parsing rely on.
pub const META_STARTS_WITH_CONCLUSION: Sentinel = Sentinel {
    id: "SYN-002",
    statement: "meta_observations starts with \"Conclusion:\".",
};

/// ING-001 — lenient ingest is total (bot-lifecycle spec Part 2): any
/// response body yields a stored result with a closed-set kind, never a
/// dispatch error. The runtime check guards the closed set so a new kind
/// cannot ship without being inventoried here.
pub const ING_NEVER_REJECTS: Sentinel = Sentinel {
    id: "ING-001",
    statement: "Ingest is total: any response body yields a stored result with a closed-set kind, never a dispatch error.",
};

/// All sentinels this crate defines (inventoried in docs/sentinels.md).
pub const SENTINELS: [Sentinel; 5] = [
    EXTRACTION_SOURCE_CLOSED_SET,
    EXTRACTED_IMPLIES_VERIFIED_QUOTE,
    CRUX_REACHES_ARTIFACT,
    META_STARTS_WITH_CONCLUSION,
    ING_NEVER_REJECTS,
];

/// Check the extraction sentinels for one provenance record against the
/// bot's raw response text. Empty vec = invariants hold.
#[must_use]
pub fn check_provenance(provenance: &FieldProvenance, raw_response: &str) -> Vec<Violation> {
    let mut out = Vec::new();
    if !matches!(
        provenance.source,
        "authored" | "extracted" | "extraction_failed"
    ) {
        out.push(Violation::of(
            &EXTRACTION_SOURCE_CLOSED_SET,
            format!(
                "field {}: unknown provenance source {:?}",
                provenance.field, provenance.source
            ),
        ));
    }
    if provenance.source == "extracted" {
        match provenance.quote.as_deref() {
            Some(quote) if crate::extractor::verify::quote_is_substring_of(quote, raw_response) => {
            }
            Some(_) => out.push(Violation::of(
                &EXTRACTED_IMPLIES_VERIFIED_QUOTE,
                format!(
                    "field {}: quote is not a verbatim substring of the raw response",
                    provenance.field
                ),
            )),
            None => out.push(Violation::of(
                &EXTRACTED_IMPLIES_VERIFIED_QUOTE,
                format!("field {}: extracted without a quote", provenance.field),
            )),
        }
    }
    out
}

/// Check the ingest sentinel: the persisted kind must come from the
/// closed set (see [`ING_NEVER_REJECTS`]).
#[must_use]
pub fn check_ingest_kind(kind: &str) -> Vec<Violation> {
    if matches!(
        kind,
        "clean" | "salvaged_field" | "salvaged_raw" | "truncated"
    ) {
        Vec::new()
    } else {
        vec![Violation::of(
            &ING_NEVER_REJECTS,
            format!("unknown ingest kind {kind:?}"),
        )]
    }
}

/// Check the artifact sentinels on a synthesis result. `crux_selected` is
/// whether crux selection succeeded for this session.
#[must_use]
pub fn check_artifact(artifact: &SessionArtifact, crux_selected: bool) -> Vec<Violation> {
    let mut out = Vec::new();
    let crux_count = artifact.issues.iter().filter(|i| i.is_crux).count();
    if crux_selected && crux_count != 1 {
        out.push(Violation::of(
            &CRUX_REACHES_ARTIFACT,
            format!("crux selected but artifact has {crux_count} is_crux issues (want 1)"),
        ));
    }
    if !artifact
        .meta_observations
        .trim_start()
        .starts_with("Conclusion:")
    {
        out.push(Violation::of(
            &META_STARTS_WITH_CONCLUSION,
            "meta_observations does not start with \"Conclusion:\"".into(),
        ));
    }
    out
}

/// Log violations at warn with the stable ID as a structured field. Callers
/// decide nothing: sentinels never block output, they make breakage loud.
pub fn log_violations(context: &str, violations: &[Violation]) {
    for v in violations {
        tracing::warn!(
            sentinel = v.sentinel_id,
            detail = %v.detail,
            "sentinel violation in {context}"
        );
    }
}

/// Render the committed inventory (docs/sentinels.md). A freshness test
/// compares this against the file; regenerate with:
/// `cargo test --test sentinels_test regen_inventory -- --ignored`
#[must_use]
pub fn render_inventory() -> String {
    let mut s = String::from(
        "# Sentinel Inventory\n\n\
         GENERATED FILE — do not edit by hand. Regenerate with:\n\
         `cargo test --test sentinels_test regen_inventory -- --ignored`\n\
         (a test fails when this file is stale).\n\n\
         A sentinel is a named runtime invariant. Violations surface as data\n\
         and warn-level logs with the stable ID — never panics, never blocked\n\
         output.\n\n\
         | ID | Invariant |\n|---|---|\n",
    );
    for sentinel in SENTINELS {
        s.push_str(&format!("| {} | {} |\n", sentinel.id, sentinel.statement));
    }
    s
}
