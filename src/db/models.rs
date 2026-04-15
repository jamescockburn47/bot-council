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
    pub confidence: Option<i64>,
    pub challenge_json: Option<String>,
    pub position_change_json: Option<String>,
    pub valid: bool,
    pub retry_count: i64,
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

/// A round's state within a debate.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoundRow {
    pub debate_id: String,
    pub round_number: i64,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// An analysis result (challenge validation, divergence, pairing).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AnalysisRow {
    pub id: String,
    pub debate_id: String,
    pub bot_id: Option<String>,
    pub analysis_type: String,
    pub input_json: String,
    pub result_json: String,
    pub model_used: String,
    pub created_at: String,
}

/// Cross-examination pairing for Round 3.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PairingRow {
    pub debate_id: String,
    pub bot_a_id: String,
    pub bot_b_id: String,
    pub third_id: Option<String>,
    pub pairing_json: String,
}

/// Final synthesis output.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SynthesisRow {
    pub debate_id: String,
    pub output_json: String,
    pub model_used: String,
    pub prompt_hash: String,
    pub created_at: String,
}

/// Role rotation history entry.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoleHistoryRow {
    pub bot_id: String,
    pub debate_id: String,
    pub role: String,
}

/// Extended debate_bots row with role column.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DebateBotWithRoleRow {
    pub debate_id: String,
    pub bot_id: String,
    pub pseudonym: String,
    pub role: Option<String>,
}
