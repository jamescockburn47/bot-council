//! Post-synthesis citation validation.
//!
//! Parses `[Agent X, Round N]` citations in synthesis output and verifies
//! each one corresponds to an actual (non-abstained) response in the transcript.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Result of citation validation.
#[derive(Debug, Serialize, Deserialize)]
pub struct CitationCheckResult {
    /// Total number of citations found in the synthesis.
    pub citations_total: usize,
    /// Number of citations that mapped to a real, non-abstained response.
    pub citations_valid: usize,
    /// Details of each citation that failed validation.
    pub citations_invalid: Vec<InvalidCitation>,
}

/// A citation that failed validation.
#[derive(Debug, Serialize, Deserialize)]
pub struct InvalidCitation {
    /// The citation text as it appeared in the synthesis.
    pub citation: String,
    /// Why the citation is invalid.
    pub reason: String,
    /// JSON path location within the synthesis where the citation was found.
    pub location: String,
}

/// Check all citations in a synthesis JSON value.
///
/// `valid_pseudonyms`: set of pseudonyms that participated (e.g. {"Agent A", "Agent B", ...})
/// `responses`: map of (pseudonym, round_number) -> abstained flag. Present = responded.
/// `max_round`: highest round number in the debate (typically 4).
pub fn check_citations(
    synthesis: &serde_json::Value,
    valid_pseudonyms: &HashSet<String>,
    responses: &HashMap<(String, i64), bool>,
    max_round: i64,
) -> CitationCheckResult {
    let re = Regex::new(r"\[([^,\]]+),\s*Round\s*(\d+)\]").expect("valid regex");
    let mut total = 0usize;
    let mut invalid = Vec::new();

    // Check consensus_points[].evidence
    if let Some(arr) = synthesis.get("consensus_points").and_then(|v| v.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            if let Some(text) = item.get("evidence").and_then(|v| v.as_str()) {
                check_text(
                    &re, text, &format!("consensus_points[{}].evidence", i),
                    valid_pseudonyms, responses, max_round, &mut total, &mut invalid,
                );
            }
        }
    }

    // Check live_disagreements[].side_a.best_argument and side_b.best_argument
    if let Some(arr) = synthesis.get("live_disagreements").and_then(|v| v.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            for side in &["side_a", "side_b"] {
                if let Some(text) = item.get(side)
                    .and_then(|s| s.get("best_argument"))
                    .and_then(|v| v.as_str())
                {
                    check_text(
                        &re, text,
                        &format!("live_disagreements[{}].{}.best_argument", i, side),
                        valid_pseudonyms, responses, max_round, &mut total, &mut invalid,
                    );
                }
            }
        }
    }

    // Check minority_positions[].key_argument
    if let Some(arr) = synthesis.get("minority_positions").and_then(|v| v.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            if let Some(text) = item.get("key_argument").and_then(|v| v.as_str()) {
                check_text(
                    &re, text, &format!("minority_positions[{}].key_argument", i),
                    valid_pseudonyms, responses, max_round, &mut total, &mut invalid,
                );
            }
        }
    }

    // Check flagged_capitulations[].flag_reason (may contain citations)
    if let Some(arr) = synthesis.get("flagged_capitulations").and_then(|v| v.as_array()) {
        for (i, item) in arr.iter().enumerate() {
            if let Some(text) = item.get("flag_reason").and_then(|v| v.as_str()) {
                check_text(
                    &re, text, &format!("flagged_capitulations[{}].flag_reason", i),
                    valid_pseudonyms, responses, max_round, &mut total, &mut invalid,
                );
            }
        }
    }

    let valid_count = total - invalid.len();
    CitationCheckResult {
        citations_total: total,
        citations_valid: valid_count,
        citations_invalid: invalid,
    }
}

/// Scan a text field for `[Agent X, Round N]` citations and validate each.
fn check_text(
    re: &Regex,
    text: &str,
    location: &str,
    valid_pseudonyms: &HashSet<String>,
    responses: &HashMap<(String, i64), bool>,
    max_round: i64,
    total: &mut usize,
    invalid: &mut Vec<InvalidCitation>,
) {
    for cap in re.captures_iter(text) {
        *total += 1;
        let agent = cap[1].trim().to_string();
        let round: i64 = match cap[2].parse() {
            Ok(r) => r,
            Err(_) => {
                invalid.push(InvalidCitation {
                    citation: format!("[{}, Round {}]", &cap[1], &cap[2]),
                    reason: "Invalid round number".into(),
                    location: location.into(),
                });
                continue;
            }
        };

        let citation_str = format!("[{}, Round {}]", agent, round);

        if !valid_pseudonyms.contains(&agent) {
            invalid.push(InvalidCitation {
                citation: citation_str,
                reason: format!("{} is not a participant", agent),
                location: location.into(),
            });
        } else if round < 0 || round > max_round {
            invalid.push(InvalidCitation {
                citation: citation_str,
                reason: format!("Round {} does not exist (max: {})", round, max_round),
                location: location.into(),
            });
        } else {
            match responses.get(&(agent.clone(), round)) {
                Some(&abstained) if abstained => {
                    invalid.push(InvalidCitation {
                        citation: citation_str,
                        reason: format!("{} abstained in Round {}", agent, round),
                        location: location.into(),
                    });
                }
                None => {
                    invalid.push(InvalidCitation {
                        citation: citation_str,
                        reason: format!("{} has no response in Round {}", agent, round),
                        location: location.into(),
                    });
                }
                _ => {} // Valid citation
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_citations() {
        let synthesis = serde_json::json!({
            "consensus_points": [{
                "point": "All agree on X",
                "supporting_bots": ["Agent A"],
                "evidence": "As stated by [Agent A, Round 2] and confirmed by [Agent B, Round 4]"
            }],
            "live_disagreements": [],
            "minority_positions": [],
            "flagged_capitulations": []
        });

        let mut pseudonyms = HashSet::new();
        pseudonyms.insert("Agent A".to_string());
        pseudonyms.insert("Agent B".to_string());

        let mut responses = HashMap::new();
        responses.insert(("Agent A".to_string(), 2), false);
        responses.insert(("Agent B".to_string(), 4), false);

        let result = check_citations(&synthesis, &pseudonyms, &responses, 4);
        assert_eq!(result.citations_total, 2);
        assert_eq!(result.citations_valid, 2);
        assert!(result.citations_invalid.is_empty());
    }

    #[test]
    fn test_invalid_pseudonym_and_round() {
        let synthesis = serde_json::json!({
            "consensus_points": [{
                "point": "Bad ref",
                "supporting_bots": ["Agent A"],
                "evidence": "[Agent Z, Round 2] said something and [Agent A, Round 9] agreed"
            }],
            "live_disagreements": [],
            "minority_positions": [],
            "flagged_capitulations": []
        });

        let mut pseudonyms = HashSet::new();
        pseudonyms.insert("Agent A".to_string());

        let mut responses = HashMap::new();
        responses.insert(("Agent A".to_string(), 0), false);

        let result = check_citations(&synthesis, &pseudonyms, &responses, 4);
        assert_eq!(result.citations_total, 2);
        assert_eq!(result.citations_valid, 0);
        assert_eq!(result.citations_invalid.len(), 2);
        assert!(result.citations_invalid[0].reason.contains("not a participant"));
        assert!(result.citations_invalid[1].reason.contains("does not exist"));
    }

    #[test]
    fn test_abstained_citation() {
        let synthesis = serde_json::json!({
            "consensus_points": [{
                "point": "Ref to abstainer",
                "supporting_bots": ["Agent A"],
                "evidence": "[Agent A, Round 1] abstained but is cited"
            }],
            "live_disagreements": [],
            "minority_positions": [],
            "flagged_capitulations": []
        });

        let mut pseudonyms = HashSet::new();
        pseudonyms.insert("Agent A".to_string());

        let mut responses = HashMap::new();
        responses.insert(("Agent A".to_string(), 1), true); // abstained = true

        let result = check_citations(&synthesis, &pseudonyms, &responses, 4);
        assert_eq!(result.citations_total, 1);
        assert_eq!(result.citations_valid, 0);
        assert_eq!(result.citations_invalid.len(), 1);
        assert!(result.citations_invalid[0].reason.contains("abstained"));
    }

    #[test]
    fn test_no_response_citation() {
        let synthesis = serde_json::json!({
            "consensus_points": [{
                "point": "Ref to missing round",
                "supporting_bots": ["Agent A"],
                "evidence": "[Agent A, Round 3] has no response entry"
            }],
            "live_disagreements": [],
            "minority_positions": [],
            "flagged_capitulations": []
        });

        let mut pseudonyms = HashSet::new();
        pseudonyms.insert("Agent A".to_string());

        let mut responses = HashMap::new();
        responses.insert(("Agent A".to_string(), 1), false);
        // Round 3 not present in responses

        let result = check_citations(&synthesis, &pseudonyms, &responses, 4);
        assert_eq!(result.citations_total, 1);
        assert_eq!(result.citations_valid, 0);
        assert_eq!(result.citations_invalid.len(), 1);
        assert!(result.citations_invalid[0].reason.contains("no response"));
    }

    #[test]
    fn test_live_disagreements_citations() {
        let synthesis = serde_json::json!({
            "consensus_points": [],
            "live_disagreements": [{
                "issue": "Topic X",
                "side_a": {
                    "position": "For",
                    "bots": ["Agent A"],
                    "best_argument": "Argued by [Agent A, Round 2]"
                },
                "side_b": {
                    "position": "Against",
                    "bots": ["Agent B"],
                    "best_argument": "Countered by [Agent B, Round 3]"
                }
            }],
            "minority_positions": [],
            "flagged_capitulations": []
        });

        let mut pseudonyms = HashSet::new();
        pseudonyms.insert("Agent A".to_string());
        pseudonyms.insert("Agent B".to_string());

        let mut responses = HashMap::new();
        responses.insert(("Agent A".to_string(), 2), false);
        responses.insert(("Agent B".to_string(), 3), false);

        let result = check_citations(&synthesis, &pseudonyms, &responses, 4);
        assert_eq!(result.citations_total, 2);
        assert_eq!(result.citations_valid, 2);
        assert!(result.citations_invalid.is_empty());
    }

    #[test]
    fn test_empty_synthesis() {
        let synthesis = serde_json::json!({});
        let pseudonyms = HashSet::new();
        let responses = HashMap::new();

        let result = check_citations(&synthesis, &pseudonyms, &responses, 4);
        assert_eq!(result.citations_total, 0);
        assert_eq!(result.citations_valid, 0);
        assert!(result.citations_invalid.is_empty());
    }
}
