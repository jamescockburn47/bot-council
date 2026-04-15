use std::collections::HashMap;
use crate::db::models::ResponseRow;
use crate::bot_client::{ChallengeField, PositionChangeField};
use serde::Serialize;

/// Pre-computed structural data fed to the synthesis prompt.
#[derive(Debug, Serialize)]
pub struct PrecomputedData {
    /// Per-pseudonym confidence values across rounds 0–4.
    pub confidence_trajectories: HashMap<String, Vec<Option<i64>>>,
    /// Position-change declarations extracted from Round 4 responses.
    pub position_changes: Vec<PositionChangeSummary>,
    /// Challenge graph extracted from Round 2 responses.
    pub challenge_graph: Vec<ChallengeSummary>,
}

/// Summary of a bot's position-change declaration.
#[derive(Debug, Serialize)]
pub struct PositionChangeSummary {
    /// Bot pseudonym.
    pub pseudonym: String,
    /// Whether the bot declared a position change.
    pub changed: bool,
    /// Summary of the prior position.
    pub from_summary: String,
    /// Summary of the new position.
    pub to_summary: String,
    /// Stated reason for the change.
    pub reason: String,
}

/// Summary of a single challenge in the challenge graph.
#[derive(Debug, Serialize)]
pub struct ChallengeSummary {
    /// Pseudonym of the challenging bot.
    pub challenger_pseudonym: String,
    /// The claim that was targeted.
    pub claim_targeted: String,
    /// The type of challenge issued.
    pub challenge_type: String,
}

/// Compute structural data from stored responses.
///
/// `pseudonym_map` maps bot_id → pseudonym.
pub fn precompute(
    responses: &[ResponseRow],
    pseudonym_map: &HashMap<String, String>,
) -> PrecomputedData {
    let trajectories = build_trajectories(responses, pseudonym_map);
    let position_changes = build_position_changes(responses, pseudonym_map);
    let challenge_graph = build_challenge_graph(responses, pseudonym_map);

    PrecomputedData { confidence_trajectories: trajectories, position_changes, challenge_graph }
}

/// Build per-pseudonym confidence trajectories across rounds 0–4.
fn build_trajectories(
    responses: &[ResponseRow],
    pseudonym_map: &HashMap<String, String>,
) -> HashMap<String, Vec<Option<i64>>> {
    let mut trajectories: HashMap<String, Vec<Option<i64>>> = HashMap::new();
    for resp in responses {
        let pseudonym = pseudonym_map.get(&resp.bot_id).cloned().unwrap_or_default();
        let entry = trajectories.entry(pseudonym).or_insert_with(|| vec![None; 5]);
        if (resp.round_number as usize) < 5 {
            entry[resp.round_number as usize] = resp.confidence;
        }
    }
    trajectories
}

/// Extract position-change declarations from Round 4 responses.
fn build_position_changes(
    responses: &[ResponseRow],
    pseudonym_map: &HashMap<String, String>,
) -> Vec<PositionChangeSummary> {
    let mut out = Vec::new();
    for resp in responses.iter().filter(|r| r.round_number == 4) {
        let pseudonym = pseudonym_map.get(&resp.bot_id).cloned().unwrap_or_default();
        if let Some(ref pc_json) = resp.position_change_json {
            match serde_json::from_str::<PositionChangeField>(pc_json) {
                Ok(pc) => out.push(PositionChangeSummary {
                    pseudonym,
                    changed: pc.changed,
                    from_summary: pc.from_summary,
                    to_summary: pc.to_summary,
                    reason: pc.reason,
                }),
                Err(e) => {
                    tracing::warn!(bot_id = %resp.bot_id, error = %e, "Failed to parse position_change_json");
                }
            }
        }
    }
    out
}

/// Extract challenge entries from Round 2 responses.
fn build_challenge_graph(
    responses: &[ResponseRow],
    pseudonym_map: &HashMap<String, String>,
) -> Vec<ChallengeSummary> {
    let mut out = Vec::new();
    for resp in responses.iter().filter(|r| r.round_number == 2) {
        let pseudonym = pseudonym_map.get(&resp.bot_id).cloned().unwrap_or_default();
        if let Some(ref cj) = resp.challenge_json {
            match serde_json::from_str::<ChallengeField>(cj) {
                Ok(c) => out.push(ChallengeSummary {
                    challenger_pseudonym: pseudonym,
                    claim_targeted: c.claim_targeted,
                    challenge_type: c.challenge_type,
                }),
                Err(e) => {
                    tracing::warn!(bot_id = %resp.bot_id, error = %e, "Failed to parse challenge_json");
                }
            }
        }
    }
    out
}
