use crate::analyser::call_minimax;
use crate::config::ModelsConfig;
use crate::sanitise::{ANTI_INJECTION_PREAMBLE, frame_untrusted};
use serde::{Deserialize, Serialize};

/// Classification of how a bot's stance on the selected crux claim moved
/// between R1 (pre-crux) and R3 (crux engagement).
///
/// Populated only when both the crux claim and the bot's R3 response text
/// are available — historical debates and crux-selection failures leave
/// `DivergenceResult::crux_shift` as `None`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CruxShift {
    /// Bot moved toward the crux claim between R1 and R3.
    ResolvedTowardCrux,
    /// Bot moved away from / strengthened opposition to the crux claim.
    ResolvedAgainstCrux,
    /// Bot held its R1 position on the crux claim.
    Unchanged,
    /// Bot explicitly rejected the crux framing.
    FrameRejected,
    /// Bot did not substantively engage the crux (abstention,
    /// carry-forward, or evasion).
    NoEngagement,
}

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
    /// How the bot's stance on the selected crux claim moved between R1
    /// and R3. `None` when no crux was selected for this debate, or when
    /// the caller did not supply the bot's R3 text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crux_shift: Option<CruxShift>,
}

/// Compare a bot's Round 0 and Round 4 positions using MiniMax.
///
/// Evaluates whether the position shifted, the magnitude, what changed, and
/// whether the participant's self-declared justification is adequate.
///
/// When both `crux_claim` and `bot_r3_text` are `Some`, the prompt also
/// asks MiniMax to classify the bot's R3 engagement with the crux; the
/// result lands in `DivergenceResult::crux_shift`. If either is `None`,
/// the crux sub-prompt is omitted and `crux_shift` stays `None` on the
/// returned value (and is dropped from the serialised JSON).
pub async fn analyse_divergence(
    config: &ModelsConfig,
    round0_response: &str,
    round4_response: &str,
    position_change_json: &str,
    crux_claim: Option<&str>,
    bot_r3_text: Option<&str>,
) -> Result<DivergenceResult, String> {
    let framed_r0 = frame_untrusted("round0_position", round0_response);
    let framed_r4 = frame_untrusted("round4_position", round4_response);
    let framed_pc = frame_untrusted("position_change", position_change_json);

    let include_crux = matches!((crux_claim, bot_r3_text), (Some(c), Some(r)) if !c.trim().is_empty() && !r.trim().is_empty());

    let prompt = if include_crux {
        let claim = crux_claim.unwrap_or("");
        let r3_text = bot_r3_text.unwrap_or("");
        let framed_crux = frame_untrusted("crux_claim", claim);
        let framed_r3 = frame_untrusted("round3_position", r3_text);
        format!(
            "{ANTI_INJECTION_PREAMBLE}\n\n\
             Compare these two positions from the same participant in a structured debate.\n\n\
             {framed_r0}\n\
             {framed_r4}\n\
             {framed_pc}\n\n\
             {framed_crux}\n\
             {framed_r3}\n\n\
             Assess:\n\
             1. Did the position substantively shift? (not just rephrasing)\n\
             2. Magnitude: none | minor | major | reversal\n\
             3. What specifically changed?\n\
             4. Is the participant's self-declared justification adequate — does it cite a specific argument from the debate that accounts for the shift?\n\
             5. Any flags (e.g., shift without justification, claimed no change but position clearly different)\n\
             6. crux_shift — classify how the participant's stance on the crux_claim moved between their earlier position and their round3_position. Pick exactly one of:\n\
                - \"resolved_toward_crux\": moved toward accepting / endorsing the crux claim\n\
                - \"resolved_against_crux\": moved away from it / strengthened opposition\n\
                - \"unchanged\": held the same stance on the crux claim\n\
                - \"frame_rejected\": explicitly rejected the framing of the crux claim itself\n\
                - \"no_engagement\": did not substantively engage the crux (abstention, carry-forward, evasion)\n\n\
             Return JSON: {{ \"shifted\": bool, \"magnitude\": \"string\", \"what_changed\": \"string\", \
             \"justification_adequate\": bool, \"flags\": [\"string\"], \"crux_shift\": \"string\" }}"
        )
    } else {
        format!(
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
        )
    };

    let result = call_minimax(config, &prompt).await?;
    serde_json::from_str::<DivergenceResult>(&result)
        .map_err(|e| format!("failed to parse divergence result: {e}, raw: {result}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crux_shift_enum_serialises_snake_case() {
        let s = serde_json::to_string(&CruxShift::ResolvedAgainstCrux).unwrap();
        assert_eq!(s, "\"resolved_against_crux\"");
        let s = serde_json::to_string(&CruxShift::FrameRejected).unwrap();
        assert_eq!(s, "\"frame_rejected\"");
        let s = serde_json::to_string(&CruxShift::ResolvedTowardCrux).unwrap();
        assert_eq!(s, "\"resolved_toward_crux\"");
        let s = serde_json::to_string(&CruxShift::Unchanged).unwrap();
        assert_eq!(s, "\"unchanged\"");
        let s = serde_json::to_string(&CruxShift::NoEngagement).unwrap();
        assert_eq!(s, "\"no_engagement\"");
    }

    #[test]
    fn divergence_result_omits_crux_shift_when_none() {
        let dr = DivergenceResult {
            shifted: false,
            magnitude: "none".into(),
            what_changed: "nothing".into(),
            justification_adequate: true,
            flags: vec![],
            crux_shift: None,
        };
        let s = serde_json::to_string(&dr).unwrap();
        assert!(
            !s.contains("crux_shift"),
            "crux_shift should be omitted when None, got: {s}"
        );
    }

    #[test]
    fn divergence_result_includes_crux_shift_when_some() {
        let dr = DivergenceResult {
            shifted: true,
            magnitude: "minor".into(),
            what_changed: "softened tone".into(),
            justification_adequate: false,
            flags: vec!["unjustified_shift".into()],
            crux_shift: Some(CruxShift::Unchanged),
        };
        let s = serde_json::to_string(&dr).unwrap();
        assert!(s.contains("crux_shift"), "missing crux_shift field: {s}");
        assert!(s.contains("unchanged"), "missing snake_case value: {s}");
    }

    #[test]
    fn divergence_result_deserialises_without_crux_shift() {
        // Older historical divergence rows persisted without the field —
        // must still round-trip cleanly via #[serde(default)].
        let raw = r#"{
            "shifted": false,
            "magnitude": "none",
            "what_changed": "no change",
            "justification_adequate": true,
            "flags": []
        }"#;
        let dr: DivergenceResult =
            serde_json::from_str(raw).expect("legacy JSON must still parse");
        assert!(dr.crux_shift.is_none());
    }
}
