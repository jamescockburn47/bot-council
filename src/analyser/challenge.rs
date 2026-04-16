use serde::Deserialize;
use crate::analyser::call_minimax;
use crate::config::ModelsConfig;
use crate::sanitise::{frame_untrusted, ANTI_INJECTION_PREAMBLE};

/// Result of MiniMax challenge validation.
#[derive(Debug, Deserialize)]
pub struct ChallengeValidation {
    /// Whether the challenge is substantive (not a vacuous restatement).
    pub valid: bool,
    /// Human-readable explanation of the validation decision.
    pub reason: String,
}

/// Validate a structured challenge using MiniMax.
///
/// Returns whether the challenge contains a specific factual claim, logical
/// objection, or premise critique directed at a named claim from another
/// participant — i.e., is not a vacuous restatement.
pub async fn validate_challenge(
    config: &ModelsConfig,
    challenge_json: &str,
    round2_response: &str,
) -> Result<ChallengeValidation, String> {
    let framed_challenge = frame_untrusted("challenge", challenge_json);
    let framed_context = frame_untrusted("round2_response", round2_response);
    let prompt = format!(
        "{ANTI_INJECTION_PREAMBLE}\n\n\
         Does the following challenge contain a specific factual claim, logical objection, \
         or premise critique directed at a named claim from another participant? \
         Return JSON: {{ \"valid\": bool, \"reason\": \"string\" }}\n\n\
         {framed_challenge}\n\
         {framed_context}"
    );

    let result = call_minimax(config, &prompt).await?;
    serde_json::from_str::<ChallengeValidation>(&result)
        .map_err(|e| format!("failed to parse challenge validation: {e}, raw: {result}"))
}
