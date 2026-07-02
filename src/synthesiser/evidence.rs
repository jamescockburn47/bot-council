//! Transcript / grounding-evidence parsing shared by prompt, meta, and
//! the synthesis pipeline.

use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;

/// One `[Agent X, Round N]: text` entry parsed from the anonymised transcript.
#[derive(Debug, Clone)]
pub(crate) struct TranscriptEntry {
    pub(crate) agent: String,
    pub(crate) round: i64,
    pub(crate) text: String,
}

/// One row of the grounding-evidence array supplied to synthesis.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GroundingEvidenceEntry {
    pub(crate) agent: String,
    pub(crate) round: i64,
    #[serde(default)]
    pub(crate) abstained: bool,
    #[serde(default)]
    pub(crate) effective_abstained: bool,
    #[serde(default = "default_valid_true")]
    pub(crate) valid: bool,
    #[serde(default)]
    pub(crate) response: String,
}

fn default_valid_true() -> bool {
    true
}

pub(crate) fn parse_grounding_evidence(
    grounding_evidence_json: &str,
) -> Vec<GroundingEvidenceEntry> {
    let rows = serde_json::from_str::<Vec<GroundingEvidenceEntry>>(grounding_evidence_json)
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| !entry.agent.trim().is_empty() && entry.round >= 0)
        .collect::<Vec<_>>();
    let mut by_round: HashMap<(String, i64), GroundingEvidenceEntry> = HashMap::new();
    for row in rows {
        by_round.insert((row.agent.clone(), row.round), row);
    }
    let mut deduped = by_round.into_values().collect::<Vec<_>>();
    deduped.sort_by(|a, b| a.agent.cmp(&b.agent).then(a.round.cmp(&b.round)));
    deduped
}

/// Parse transcript lines in format `[Agent X, Round N]: response`.
pub(crate) fn parse_transcript_entries(transcript_text: &str) -> Vec<TranscriptEntry> {
    let re =
        Regex::new(r"(?m)^\[(?P<agent>[^\],]+), Round (?P<round>\d+)\]: ").expect("valid regex");
    let mut marks = Vec::new();
    for caps in re.captures_iter(transcript_text) {
        let Some(m) = caps.get(0) else { continue };
        let agent = caps
            .name("agent")
            .map(|m| m.as_str().trim().to_string())
            .unwrap_or_default();
        let round = caps
            .name("round")
            .and_then(|m| m.as_str().parse::<i64>().ok())
            .unwrap_or(0);
        marks.push((m.start(), m.end(), agent, round));
    }

    let mut out = Vec::new();
    for (idx, mark) in marks.iter().enumerate() {
        let start = mark.1;
        let end = marks
            .get(idx + 1)
            .map(|next| next.0)
            .unwrap_or(transcript_text.len());
        let text = transcript_text[start..end].trim().to_string();
        out.push(TranscriptEntry {
            agent: mark.2.clone(),
            round: mark.3,
            text,
        });
    }
    out
}

/// Parse participant map lines of form `Agent A = Clint`.
pub(crate) fn parse_participant_map(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some((left, right)) = trimmed.split_once('=') {
            let key = left.trim().to_string();
            let value = right.trim().to_string();
            if !key.is_empty() && !value.is_empty() {
                map.insert(key, value);
            }
        }
    }
    map
}

pub(crate) fn summarize_for_meta(text: &str, max_chars: usize) -> String {
    let normalized = text
        .replace("**", "")
        .replace('`', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut truncated = normalized.chars().take(max_chars).collect::<String>();
    if let Some(idx) = truncated.rfind(|c: char| c == '.' || c == '!' || c == '?' || c == ';') {
        truncated.truncate(idx + 1);
    } else if let Some(idx) = truncated.rfind(',') {
        truncated.truncate(idx);
    }
    let trimmed = truncated.trim();
    if trimmed.is_empty() {
        return "…".into();
    }
    format!("{trimmed}…")
}

pub(crate) fn would_exceed_budget(
    current_chars: usize,
    next_section: &str,
    max_chars: usize,
) -> bool {
    current_chars + next_section.len() + 2 > max_chars
}
