//! Deterministic, evidence-grounded meta_observations composition.

use crate::synthesiser::evidence::{
    parse_grounding_evidence, parse_participant_map, parse_transcript_entries, summarize_for_meta,
    GroundingEvidenceEntry,
};
use crate::synthesiser::schema::{IssueStatus, SessionArtifact};
use std::collections::HashMap;

/// Ensure meta observations are grounded in deterministic transcript evidence.
pub(crate) fn ensure_substantive_meta(
    output: &mut SessionArtifact,
    participant_map_text: &str,
    transcript_text: &str,
    grounding_evidence_json: &str,
) {
    output.meta_observations = compose_structured_meta(
        output,
        participant_map_text,
        transcript_text,
        grounding_evidence_json,
    );
    if !output
        .meta_observations
        .trim_start()
        .starts_with("Conclusion:")
    {
        output.meta_observations = format!("Conclusion: {}", output.meta_observations.trim());
    }
}

pub(crate) fn compose_structured_meta(
    output: &SessionArtifact,
    participant_map_text: &str,
    transcript_text: &str,
    grounding_evidence_json: &str,
) -> String {
    let mut sections = Vec::new();
    sections.push(format!("Conclusion: {}", derive_overall_outcome(output)));

    let settled_count = output
        .issues
        .iter()
        .filter(|i| i.status == IssueStatus::Settled)
        .count();
    let contested: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.status != IssueStatus::Settled)
        .collect();

    let summary_of_arguments = if output.issues.is_empty() {
        "- Structured issue map not extracted; see the raw transcript for per-bot positions."
            .to_string()
    } else {
        output
            .issues
            .iter()
            .take(6)
            .map(|i| {
                let status = match i.status {
                    IssueStatus::Settled => "settled",
                    IssueStatus::Split => "split",
                    IssueStatus::Reframed => "reframed",
                };
                let crux = if i.is_crux { " [crux]" } else { "" };
                format!(
                    "- {}{crux} ({status}): {}",
                    summarize_for_meta(&i.issue, 140),
                    i.positions
                        .iter()
                        .take(3)
                        .map(|p| format!(
                            "{} ({})",
                            summarize_for_meta(&p.stance, 100),
                            p.bots.join(", ")
                        ))
                        .collect::<Vec<_>>()
                        .join(" vs ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections.push(format!("Summary of arguments:\n{summary_of_arguments}"));

    let disagreements = if contested.is_empty() {
        "- No live disagreement remained at synthesis time.".to_string()
    } else {
        contested
            .iter()
            .take(4)
            .map(|i| {
                format!(
                    "- {} | {}",
                    summarize_for_meta(&i.issue, 90),
                    i.positions
                        .iter()
                        .take(3)
                        .map(|p| summarize_for_meta(&p.best_argument, 150))
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections.push(format!("Key disagreements:\n{disagreements}"));

    // Minority position = a position held by exactly one bot.
    let minorities: Vec<String> = output
        .issues
        .iter()
        .flat_map(|i| i.positions.iter())
        .filter(|p| p.bots.len() == 1)
        .take(4)
        .map(|p| {
            format!(
                "- {}: {} | {}",
                p.bots.first().cloned().unwrap_or_default(),
                summarize_for_meta(&p.stance, 120),
                summarize_for_meta(&p.best_argument, 180)
            )
        })
        .collect();
    let minority_section = if minorities.is_empty() {
        "- No explicit minority position was preserved in this run.".to_string()
    } else {
        minorities.join("\n")
    };
    sections.push(format!("Minority positions:\n{minority_section}"));

    let unjustified = output
        .issues
        .iter()
        .flat_map(|i| i.movement.iter())
        .filter(|m| !m.justified)
        .count();
    let outcome = format!(
        "- Issues: {} ({} settled, {} contested)\n- Position shifts: {} ({} flagged unjustified)",
        output.issues.len(),
        settled_count,
        contested.len(),
        output
            .issues
            .iter()
            .map(|i| i.movement.len())
            .sum::<usize>(),
        unjustified
    );
    sections.push(format!("Overall outcome:\n{outcome}"));

    let behavior = build_behavior_notes(
        participant_map_text,
        transcript_text,
        grounding_evidence_json,
    );
    sections.push(format!("Bot behaviour notes:\n{behavior}"));

    sections.join("\n\n")
}

fn derive_overall_outcome(a: &SessionArtifact) -> String {
    let settled = a
        .issues
        .iter()
        .filter(|i| i.status == IssueStatus::Settled)
        .count();
    let contested = a.issues.len() - settled;
    match (settled, contested) {
        (0, 0) => "No structured issue map was extracted; the full per-bot positions remain in the transcript.".into(),
        (_, 0) => "Broad alignment: every extracted issue settled.".into(),
        (0, _) => "No consensus: every extracted issue remained contested at close.".into(),
        _ => "Partial convergence: some issues settled, others remained contested.".into(),
    }
}

fn build_behavior_notes(
    participant_map_text: &str,
    transcript_text: &str,
    grounding_evidence_json: &str,
) -> String {
    let mut evidence = parse_grounding_evidence(grounding_evidence_json);
    if evidence.is_empty() {
        evidence = parse_transcript_entries(transcript_text)
            .into_iter()
            .map(|entry| GroundingEvidenceEntry {
                agent: entry.agent,
                round: entry.round,
                abstained: false,
                effective_abstained: false,
                valid: true,
                response: entry.text,
            })
            .collect();
    }
    if evidence.is_empty() {
        return "- No transcript evidence available for behaviour analysis.".into();
    }

    let alias_map = parse_participant_map(participant_map_text);
    let mut grouped: HashMap<String, Vec<GroundingEvidenceEntry>> = HashMap::new();
    for row in evidence {
        grouped.entry(row.agent.clone()).or_default().push(row);
    }
    let mut agents: Vec<String> = grouped.keys().cloned().collect();
    agents.sort();
    let mut lines = Vec::new();
    for agent in agents {
        let mut entries = grouped.get(&agent).cloned().unwrap_or_default();
        entries.sort_by_key(|e| e.round);
        let responded = entries
            .iter()
            .filter(|e| !e.abstained && !e.effective_abstained && e.valid)
            .count();
        let abstained = entries
            .iter()
            .filter(|e| e.abstained || e.effective_abstained)
            .count();
        let invalid = entries.iter().filter(|e| !e.valid).count();
        let label = alias_map
            .get(&agent)
            .map(|name| format!("{agent} ({name})"))
            .unwrap_or(agent);
        lines.push(format!(
            "- {label}: responded={responded}, abstained/effective-abstained={abstained}, invalid={invalid}."
        ));
        // For abstaining bots, surface the bot's own wrapper text from the
        // earliest gap round — this is the actionable diagnostic the bot
        // operator needs to fix the failure (provider error, upstream
        // timeout, rate limit, empty-response, etc.).
        if abstained > 0 {
            if let Some(first_gap) = entries
                .iter()
                .find(|e| e.abstained || e.effective_abstained)
            {
                let signal: String = first_gap
                    .response
                    .trim()
                    .replace('\n', " ")
                    .chars()
                    .take(240)
                    .collect();
                if !signal.is_empty() {
                    lines.push(format!(
                        "  Wrapper signal (first gap, Round {}): \"{}\". Operator: check upstream model availability / API key / rate limits in this bot's wrapper.",
                        first_gap.round, signal
                    ));
                }
            }
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::compose_structured_meta;
    use crate::synthesiser::schema::{Issue, IssueStatus, Position, SessionArtifact};

    #[test]
    fn structured_meta_leads_with_summary_sections() {
        let artifact = SessionArtifact {
            topic: "t".into(),
            headline: String::new(),
            executive_summary: String::new(),
            issues: vec![Issue {
                issue: "Whether identity certificates improve trust".into(),
                headline: "Certificate trust value".into(),
                is_crux: true,
                status: IssueStatus::Split,
                positions: vec![
                    Position {
                        stance: "Certificates materially improve trust".into(),
                        headline: "Certificates improve trust".into(),
                        bots: vec!["Agent A".into()],
                        best_argument:
                            "Trust improves when attestations are verifiable [Agent A, Round 2]"
                                .into(),
                        evidence: String::new(),
                        final_confidence: Some(70),
                        frame_rejection: false,
                    },
                    Position {
                        stance: "Keep identity optional and audit controls mandatory".into(),
                        headline: "Audit over identity".into(),
                        bots: vec!["Agent C".into()],
                        best_argument:
                            "Mandatory identity can be theatre without enforcement [Agent C, Round 2]"
                                .into(),
                        evidence: String::new(),
                        final_confidence: Some(62),
                        frame_rejection: false,
                    },
                ],
                movement: vec![],
            }],
            meta_observations: String::new(),
        };
        let evidence = serde_json::json!([
            {"agent":"Agent A","round":0,"abstained":false,"valid":true,"response":"opening"},
            {"agent":"Agent C","round":0,"abstained":false,"valid":true,"response":"opening"}
        ])
        .to_string();

        let meta = compose_structured_meta(
            &artifact,
            "Agent A = Alice\nAgent C = Cara",
            "",
            &evidence,
        );
        assert!(meta.starts_with("Conclusion:"));
        assert!(meta.contains("Summary of arguments:"));
        assert!(meta.contains("[crux]"));
        assert!(meta.contains("Key disagreements:"));
        assert!(meta.contains("Minority positions:"));
        assert!(meta.contains("Audit over identity") || meta.contains("Agent C"));
        assert!(meta.contains("Overall outcome:"));
        assert!(meta.contains("Bot behaviour notes:"));
    }

    #[test]
    fn behavior_notes_track_abstentions_and_wrapper_signal() {
        let evidence = serde_json::json!([
            {"agent":"Agent B","round":0,"abstained":false,"valid":true,"response":"Opening."},
            {"agent":"Agent B","round":1,"abstained":true,"valid":true,"response":"provider call failed"}
        ])
        .to_string();
        let notes = super::build_behavior_notes("", "", &evidence);
        assert!(notes.contains("abstained/effective-abstained=1"));
        assert!(notes.contains("provider call failed"));
    }
}
