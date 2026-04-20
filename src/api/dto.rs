use serde::{Deserialize, Serialize};

/// Request body for registering a new bot.
#[derive(Debug, Deserialize)]
pub struct CreateBotRequest {
    pub name: String,
    pub endpoint_url: String,
    #[serde(default)]
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
    pub performance: Option<BotPerformanceSummary>,
}

/// Aggregated performance summary for a bot.
#[derive(Debug, Serialize, Clone)]
pub struct BotPerformanceSummary {
    pub score_out_of_10: f64,
    pub critical_thinking_score_out_of_10: f64,
    pub resource_use_score_out_of_10: f64,
    pub instruction_following_score_out_of_10: f64,
    pub functionality_score_out_of_10: f64,
    pub usefulness_score_out_of_10: f64,
    pub debate_engagement_score_out_of_10: f64,
    pub total_rounds: i64,
    pub debates_participated: i64,
    pub abstained_rounds: i64,
    pub invalid_rounds: i64,
    pub degraded_rounds: i64,
    pub last_debate_at: Option<String>,
    pub suggestions: Vec<String>,
}

/// Analytics view for a single bot.
#[derive(Debug, Serialize)]
pub struct BotAnalyticsResponse {
    pub bot: BotResponse,
    pub recent_debates: Vec<BotDebateAnalytics>,
}

/// Per-debate summary row in bot analytics.
#[derive(Debug, Serialize)]
pub struct BotDebateAnalytics {
    pub debate_id: String,
    pub topic: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub role: Option<String>,
    pub rounds_total: i64,
    pub abstained_rounds: i64,
    pub invalid_rounds: i64,
    pub degraded_rounds: i64,
}

/// Request body for rejecting a bot; requires a human-readable reason.
#[derive(Debug, Deserialize)]
pub struct RejectBotRequest {
    pub reason: String,
}

/// Response body for a manual bot endpoint health check.
#[derive(Debug, Serialize)]
pub struct BotHealthCheckResponse {
    pub ok: bool,
    pub message: String,
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
    pub test: Option<bool>,
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
