use serde::{Deserialize, Serialize};

/// Request body for registering a new bot.
#[derive(Debug, Deserialize)]
pub struct CreateBotRequest {
    pub name: String,
    pub endpoint_url: String,
    pub token: String,
    pub model_family: Option<String>,
    pub description: Option<String>,
}

/// Response body for a bot resource.
#[derive(Debug, Serialize)]
pub struct BotResponse {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub model_family: Option<String>,
    pub status: String,
    pub description: Option<String>,
    pub submitted_by: Option<String>,
    pub rejection_reason: Option<String>,
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
    pub created_at: String,
}

/// Request body for rejecting a bot; requires a human-readable reason.
#[derive(Debug, Deserialize)]
pub struct RejectBotRequest {
    pub reason: String,
}

/// Response body for GET /me.
#[derive(Debug, Serialize)]
pub struct UserInfoResponse {
    pub user_id: String,
    pub role: String,
}

/// Request body for creating a new debate.
#[derive(Debug, Deserialize)]
pub struct CreateDebateRequest {
    pub topic: String,
    pub bot_ids: Option<Vec<String>>,
}

/// Response body for a debate resource.
#[derive(Debug, Serialize)]
pub struct DebateResponse {
    pub id: String,
    pub topic: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub bots: Vec<DebateBotInfo>,
    pub results: Option<DebateResults>,
}

/// Bot assignment within a debate (pseudonymised, with optional role).
#[derive(Debug, Serialize)]
pub struct DebateBotInfo {
    pub bot_id: String,
    pub bot_name: String,
    pub pseudonym: String,
    pub role: Option<String>,
}

/// Aggregated results for a completed debate.
#[derive(Debug, Serialize)]
pub struct DebateResults {
    pub responses: Vec<AnonymisedResponse>,
    pub rankings: Vec<RankedArgument>,
}

/// A single anonymised response from a debate round.
#[derive(Debug, Serialize)]
pub struct AnonymisedResponse {
    pub pseudonym: String,
    pub response: String,
    pub abstained: bool,
}

/// Aggregated peer-scoring rankings for a bot in a debate.
#[derive(Debug, Serialize)]
pub struct RankedArgument {
    pub pseudonym: String,
    pub avg_reasoning_quality: f64,
    pub avg_factual_grounding: f64,
    pub avg_overall: f64,
    pub total_scores: usize,
}

/// Query parameters for listing debates.
#[derive(Debug, Deserialize)]
pub struct ListDebatesQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// Response for GET /debates/{id}/transcript.
#[derive(Debug, Serialize)]
pub struct TranscriptResponse {
    pub debate_id: String,
    pub topic: String,
    pub rounds: Vec<TranscriptRound>,
    pub anonymisation_log: Vec<AnonymisationEntry>,
    pub divergence_analyses: Vec<DivergenceEntry>,
}

/// A single round in the transcript.
#[derive(Debug, Serialize)]
pub struct TranscriptRound {
    pub round_number: i64,
    pub status: String,
    pub responses: Vec<TranscriptEntry>,
}

/// A single response entry in a transcript round.
#[derive(Debug, Serialize)]
pub struct TranscriptEntry {
    pub pseudonym: String,
    pub response: String,
    pub confidence: Option<i64>,
    pub challenge: Option<serde_json::Value>,
    pub position_change: Option<serde_json::Value>,
    pub valid: bool,
    pub abstained: bool,
    pub validation_reasoning: Option<String>,
}

/// Anonymisation log entry mapping pseudonym to role.
#[derive(Debug, Serialize)]
pub struct AnonymisationEntry {
    pub pseudonym: String,
    pub role: Option<String>,
}

/// Divergence analysis entry for a bot across rounds.
#[derive(Debug, Serialize)]
pub struct DivergenceEntry {
    pub pseudonym: String,
    pub shifted: Option<bool>,
    pub magnitude: Option<String>,
    pub what_changed: Option<String>,
    pub justification_adequate: Option<bool>,
    pub flags: Vec<String>,
}

/// Response for GET /debates/{id}/synthesis.
#[derive(Debug, Serialize)]
pub struct SynthesisResponse {
    pub debate_id: String,
    pub synthesis: serde_json::Value,
    pub model_used: String,
    pub created_at: String,
    pub citation_check: Option<serde_json::Value>,
}

/// Request body for POST /bots/validate — dry-run smoke test without
/// persisting a bot. Mirrors a subset of CreateBotRequest.
#[derive(Debug, Deserialize)]
pub struct ValidateBotRequest {
    pub endpoint_url: String,
    pub token: String,
}

/// Result of a single check during validation.
#[derive(Debug, Serialize)]
pub struct ValidateCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// Response for POST /bots/validate.
#[derive(Debug, Serialize)]
pub struct ValidateBotResponse {
    pub ok: bool,
    pub checks: Vec<ValidateCheck>,
}

/// One entry in the per-bot history endpoint.
#[derive(Debug, Serialize)]
pub struct BotHistoryEntry {
    pub debate_id: String,
    pub round_number: i64,
    pub created_at: String,
    pub valid: bool,
    pub abstained: bool,
    pub error_kind: Option<String>,
    pub error_detail: Option<String>,
    pub elapsed_ms: Option<i64>,
}

/// Response for GET /diag/health — extended health surface for admins.
#[derive(Debug, Serialize)]
pub struct DiagHealthResponse {
    /// Number of debates currently in a non-terminal status.
    pub debates_in_flight: i64,
    /// ISO-8601 timestamp of the most recent debate completion, or null
    /// if the harness has never completed a debate.
    pub last_completion_ts: Option<String>,
    /// Failure rate over the last hour (0.0–1.0); null when no debates
    /// terminated in the window.
    pub failure_rate_1h: Option<f64>,
    /// Number of debates with status 'failed' in the last hour.
    pub failures_1h: i64,
    /// Total debates that reached a terminal status in the last hour.
    pub terminal_1h: i64,
    /// Git SHA or cargo version currently running (mirrors SENTRY_RELEASE).
    pub release: String,
}
