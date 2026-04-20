use crate::analyser::call_minimax;
use crate::config::ModelsConfig;
use crate::sanitise::{ANTI_INJECTION_PREAMBLE, frame_untrusted};
use serde::{Deserialize, Serialize};

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
    let framed_r0 = frame_untrusted("round0_position", round0_response);
    let framed_r4 = frame_untrusted("round4_position", round4_response);
    let framed_pc = frame_untrusted("position_change", position_change_json);
    let prompt = format!(
        "{ANTI_INJECTION_PREAMBLE}\n\n\
         Compare these two positions from the same participant in a structured debate.\n\n\
         {framed_r0}\n\
         {framed_r4}\n\
         {framed_pc}\n\n\
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
