use serde::{Deserialize, Serialize};
use crate::analyser::call_minimax;
use crate::config::ModelsConfig;

/// Per-bot divergence analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergenceResult {
    /// Whether the position substantively shifted (not just rephrasing).
    pub shifted: bool,
    /// Magnitude of shift: none | minor | major | reversal.
    pub magnitude: String,
    /// Description of what specifically changed between rounds.
    pub what_changed: String,
    /// Whether the participant's self-declared justification adequately
    /// cites a specific debate argument that accounts for the shift.
    pub justification_adequate: bool,
    /// Any flags raised (e.g., shift without justification, claimed no change
    /// but position clearly different).
    pub flags: Vec<String>,
}

/// Compare a bot's Round 0 and Round 4 positions using MiniMax.
///
/// Evaluates whether the position shifted, the magnitude, what changed, and
/// whether the participant's self-declared justification is adequate.
pub async fn analyse_divergence(
    config: &ModelsConfig,
    round0_response: &str,
    round4_response: &str,
    position_change_json: &str,
) -> Result<DivergenceResult, String> {
    let prompt = format!(
        "Compare these two positions from the same participant in a structured debate.\n\n\
         Round 0 position: {round0_response}\n\
         Round 4 position: {round4_response}\n\
         Participant's self-declared position_change: {position_change_json}\n\n\
         Assess:\n\
         1. Did the position substantively shift? (not just rephrasing)\n\
         2. Magnitude: none | minor | major | reversal\n\
         3. What specifically changed?\n\
         4. Is the participant's self-declared justification adequate — does it cite a specific argument from the debate that accounts for the shift?\n\
         5. Any flags (e.g., shift without justification, claimed no change but position clearly different)\n\n\
         Return JSON: {{ \"shifted\": bool, \"magnitude\": \"string\", \"what_changed\": \"string\", \
         \"justification_adequate\": bool, \"flags\": [\"string\"] }}"
    );

    let result = call_minimax(config, &prompt).await?;
    serde_json::from_str::<DivergenceResult>(&result)
        .map_err(|e| format!("failed to parse divergence result: {e}, raw: {result}"))
}
