//! Sentinel validator tests (moved from src/observability/sentinels.rs to
//! keep that file under the 300-line gate; all APIs under test are public).

// Test code may unwrap/expect/panic — that is what asserts are
// (CLAUDE.md: unwrap() allowed in tests).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use bot_council::observability::sentinels::*;
use bot_council::orchestrator::extraction::FieldProvenance;
use bot_council::synthesiser::schema::{Issue, IssueStatus, SessionArtifact};

fn provenance(source: &'static str, quote: Option<&str>) -> FieldProvenance {
    FieldProvenance {
        field: "challenge",
        source,
        quote: quote.map(str::to_string),
    }
}

#[test]
fn authored_and_failed_pass() {
    assert!(check_provenance(&provenance("authored", None), "text").is_empty());
    assert!(check_provenance(&provenance("extraction_failed", None), "text").is_empty());
}

#[test]
fn unknown_source_violates_ext_001() {
    let v = check_provenance(&provenance("guessed", None), "text");
    assert!(v.iter().any(|v| v.sentinel_id == "EXT-001"));
}

#[test]
fn extracted_with_verbatim_quote_passes() {
    let raw = "the claim that preflight checks help is wrong";
    let v = check_provenance(&provenance("extracted", Some("preflight checks help")), raw);
    assert!(v.is_empty());
}

#[test]
fn extracted_with_fabricated_quote_violates_ext_002() {
    let v = check_provenance(
        &provenance("extracted", Some("never said this")),
        "the actual response",
    );
    assert!(v.iter().any(|v| v.sentinel_id == "EXT-002"));
}

#[test]
fn extracted_without_quote_violates_ext_002() {
    let v = check_provenance(&provenance("extracted", None), "text");
    assert!(v.iter().any(|v| v.sentinel_id == "EXT-002"));
}

fn artifact(issues: Vec<Issue>, meta: &str) -> SessionArtifact {
    SessionArtifact {
        topic: "t".into(),
        headline: String::new(),
        executive_summary: String::new(),
        issues,
        meta_observations: meta.into(),
    }
}

fn issue(is_crux: bool) -> Issue {
    Issue {
        issue: "q".into(),
        headline: String::new(),
        is_crux,
        status: IssueStatus::Split,
        positions: vec![],
        movement: vec![],
    }
}

#[test]
fn crux_present_and_conclusion_pass() {
    let a = artifact(vec![issue(true), issue(false)], "Conclusion: fine.");
    assert!(check_artifact(&a, true).is_empty());
}

#[test]
fn missing_crux_violates_syn_001() {
    let a = artifact(vec![issue(false)], "Conclusion: fine.");
    let v = check_artifact(&a, true);
    assert!(v.iter().any(|v| v.sentinel_id == "SYN-001"));
    // No crux selected => no requirement.
    assert!(check_artifact(&a, false).is_empty());
}

#[test]
fn bad_meta_violates_syn_002() {
    let a = artifact(vec![issue(true)], "it went fine");
    let v = check_artifact(&a, true);
    assert!(v.iter().any(|v| v.sentinel_id == "SYN-002"));
}

#[test]
fn inventory_is_fresh() {
    let committed = include_str!("../docs/sentinels.md");
    assert_eq!(
        committed,
        render_inventory(),
        "docs/sentinels.md is stale — run `cargo test sentinels::tests::regen_inventory -- --ignored`"
    );
}

#[test]
#[ignore = "writes docs/sentinels.md; run explicitly to regenerate"]
fn regen_inventory() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/sentinels.md");
    std::fs::write(path, render_inventory()).expect("write inventory");
}
