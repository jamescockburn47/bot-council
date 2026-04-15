use serde::{Deserialize, Serialize};

/// Request body for registering a new bot.
#[derive(Debug, Deserialize)]
pub struct CreateBotRequest {
    pub name: String,
    pub endpoint_url: String,
    pub token: String,
    pub model_family: Option<String>,
}

/// Response body for a bot resource.
#[derive(Debug, Serialize)]
pub struct BotResponse {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub model_family: Option<String>,
    pub active: bool,
    pub created_at: String,
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

/// Bot assignment within a debate (pseudonymised).
#[derive(Debug, Serialize)]
pub struct DebateBotInfo {
    pub bot_id: String,
    pub bot_name: String,
    pub pseudonym: String,
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
