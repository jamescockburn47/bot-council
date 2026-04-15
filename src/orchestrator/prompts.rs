use crate::types::Role;

/// Round 0: Blind formation prompt. Bot receives topic and role, no context.
pub fn round0_prompt(topic: &str, role: Role) -> String {
    format!(
        "You are participating in a structured adversarial debate.\n\
         Topic: {topic}\n\
         Your role: {} — {}\n\n\
         State your initial position on this topic. Be substantive and specific.\n\
         Do not hedge or equivocate — commit to a clear position consistent with your assigned role.",
        role.as_str(),
        role.description()
    )
}

/// Round 1: Anonymous distribution prompt. Bot sees all Round 0 positions.
pub fn round1_prompt(own_pseudonym: &str) -> String {
    format!(
        "Here are the initial positions from all participants (anonymised).\n\
         Your previous position was submitted as {own_pseudonym}.\n\n\
         Review all positions. You must:\n\
         1. Identify the single strongest argument that opposes your position and explain why it is strong.\n\
         2. State specifically what evidence or reasoning would cause you to change your position.\n\n\
         Do not agree with other positions unless you can articulate exactly why the argument compels agreement."
    )
}

/// Round 2: Structured rebuttal prompt. Mandatory challenge field.
pub fn round2_prompt() -> String {
    "Here are the Round 1 responses from all participants.\n\n\
     You must raise at least one specific challenge. Your challenge must:\n\
     - Target a specific claim made by another participant (cite the pseudonym and claim)\n\
     - Provide counter-evidence or identify a logical flaw\n\
     - Be classified as factual, logical, or premise-based\n\n\
     A response without an explicit challenge will be rejected.\n\n\
     Your response JSON must include a `challenge` object with fields:\n\
     - `claim_targeted`: the specific claim you are challenging\n\
     - `counter_evidence`: your counter-evidence or logical objection\n\
     - `type`: one of \"factual\", \"logical\", or \"premise\"".to_string()
}

/// Round 2: Re-prompt after failed challenge validation.
pub fn round2_reprompt(reason: &str) -> String {
    format!(
        "Your response was rejected: {reason}\n\n\
         You must raise at least one factual or logical objection to another participant's position. \
         Include a `challenge` object with `claim_targeted`, `counter_evidence`, and `type` fields. Resubmit."
    )
}

/// Round 3: Cross-examination question prompt (Pass A).
pub fn round3_question_prompt(partner_pseudonym: &str, partner_round2_response: &str) -> String {
    format!(
        "You are in cross-examination with {partner_pseudonym}.\n\
         Their position: {partner_round2_response}\n\n\
         Pose one pointed question to {partner_pseudonym} that surfaces a hidden assumption \
         or unstated dependency in their argument.\n\n\
         Be direct. Do not soften your question to avoid conflict."
    )
}

/// Round 3: Cross-examination answer prompt (Pass B).
pub fn round3_answer_prompt(
    partner_pseudonym: &str,
    partner_round2_response: &str,
    question_posed_to_you: &str,
) -> String {
    format!(
        "You are in cross-examination with {partner_pseudonym}.\n\
         Their position: {partner_round2_response}\n\n\
         Answer the question posed to you by {partner_pseudonym}: \"{question_posed_to_you}\"\n\n\
         Be direct and substantive."
    )
}

/// Round 4: Final position prompt.
pub fn round4_prompt(topic: &str) -> String {
    format!(
        "This is the final round. State your final position on: {topic}\n\n\
         You must include:\n\
         1. Your final position — clear, specific, and substantive.\n\
         2. A confidence score (0-100) reflecting your genuine certainty.\n\
         3. A position_change declaration: did your position change from Round 0? \
         If yes, state what changed, what it changed from, and the specific argument that caused the change. \
         If no, state why the opposing arguments were insufficient.\n\n\
         Do not soften your position for the sake of agreement. \
         Minority positions are preserved and valued in the synthesis.\n\n\
         Your response JSON must include a `position_change` object with fields:\n\
         - `changed`: boolean\n\
         - `from_summary`: your Round 0 position (brief)\n\
         - `to_summary`: your final position (brief)\n\
         - `reason`: what caused the change (or why you didn't change)"
    )
}
