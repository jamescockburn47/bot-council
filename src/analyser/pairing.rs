use serde::{Deserialize, Serialize};
use crate::analyser::call_minimax;
use crate::config::ModelsConfig;
use crate::sanitise::{frame_response, ANTI_INJECTION_PREAMBLE};

/// MiniMax pairing result — which bots to pair for cross-examination.
#[derive(Debug, Serialize, Deserialize)]
pub struct PairingResult {
    /// First cross-examination pair (two pseudonyms).
    pub pair_1: Vec<String>,
    /// Second cross-examination pair (two pseudonyms).
    pub pair_2: Vec<String>,
    /// Which pair the third participant joins ("pair_1" or "pair_2").
    pub third_joins: String,
    /// Pseudonym of the participant who joins an existing pair.
    pub third: String,
}

/// Determine cross-examination pairings based on maximum semantic divergence.
///
/// Sends all Round 2 positions to MiniMax and gets back optimal pairings.
/// The fifth participant joins whichever pair has the most similar positions,
/// creating a 3-way cross-examination group.
pub async fn compute_pairings(
    config: &ModelsConfig,
    positions: &[(String, String)], // (pseudonym, round2_response)
) -> Result<PairingResult, String> {
    let positions_text: String = positions.iter()
        .map(|(pseudo, resp)| frame_response(pseudo, resp))
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt = format!(
        "{ANTI_INJECTION_PREAMBLE}\n\n\
         Given these {} debate positions, identify the two pairs of participants whose positions \
         are most divergent. The remaining participant joins whichever pair has the most similar \
         positions (creating a 3-way). Return JSON: \
         {{ \"pair_1\": [\"Agent X\", \"Agent Y\"], \"pair_2\": [\"Agent W\", \"Agent Z\"], \
         \"third_joins\": \"pair_1\" or \"pair_2\", \"third\": \"Agent V\" }}\n\n\
         Positions:\n{positions_text}",
        positions.len()
    );

    let result = call_minimax(config, &prompt).await?;
    serde_json::from_str::<PairingResult>(&result)
        .map_err(|e| format!("failed to parse pairing result: {e}, raw: {result}"))
}
