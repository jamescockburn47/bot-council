use serde::Serialize;

/// Database row for a registered bot.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BotRow {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub token_hash: String,
    pub model_family: Option<String>,
    pub active: bool,
    pub created_at: String,
}

/// Database row for a debate session.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DebateRow {
    pub id: String,
    pub topic: String,
    pub status: String,
    pub config_json: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Join table row linking a bot to a debate with its assigned pseudonym.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DebateBotRow {
    pub debate_id: String,
    pub bot_id: String,
    pub pseudonym: String,
}

/// Database row for a single bot response within a debate round.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ResponseRow {
    pub id: String,
    pub debate_id: String,
    pub round_number: i64,
    pub bot_id: String,
    pub response_json: String,
    pub abstained: bool,
    pub created_at: String,
}

/// Database row for a peer score issued by one bot against another's pseudonym.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PeerScoreRow {
    pub id: String,
    pub debate_id: String,
    pub scorer_bot_id: String,
    pub target_pseudonym: String,
    pub reasoning_quality: i64,
    pub factual_grounding: i64,
    pub overall: i64,
    pub reasoning: String,
    pub created_at: String,
}
