use sqlx::SqlitePool;
use crate::db::models::*;

/// Insert a new bot registration and return the created row.
pub async fn insert_bot(
    pool: &SqlitePool, id: &str, name: &str, endpoint_url: &str,
    token_hash: &str, model_family: Option<&str>,
    submitted_by: Option<&str>, description: Option<&str>, status: &str,
) -> Result<BotRow, sqlx::Error> {
    sqlx::query_as::<_, BotRow>(
        "INSERT INTO bots (id, name, endpoint_url, token_hash, model_family, submitted_by, description, status) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING *"
    )
    .bind(id).bind(name).bind(endpoint_url).bind(token_hash).bind(model_family)
    .bind(submitted_by).bind(description).bind(status)
    .fetch_one(pool).await
}

/// Return all active bots ordered by creation time.
pub async fn list_active_bots(pool: &SqlitePool) -> Result<Vec<BotRow>, sqlx::Error> {
    sqlx::query_as::<_, BotRow>(
        "SELECT id, name, endpoint_url, token_hash, model_family, active, \
         status, submitted_by, description, reviewed_at, reviewed_by, created_at \
         FROM bots WHERE status = 'active' ORDER BY created_at"
    ).fetch_all(pool).await
}

/// Fetch a single bot by ID, or None if not found.
pub async fn get_bot(pool: &SqlitePool, id: &str) -> Result<Option<BotRow>, sqlx::Error> {
    sqlx::query_as::<_, BotRow>(
        "SELECT id, name, endpoint_url, token_hash, model_family, active, \
         status, submitted_by, description, reviewed_at, reviewed_by, created_at \
         FROM bots WHERE id = ?"
    ).bind(id).fetch_optional(pool).await
}

/// Fetch multiple active bots by a slice of IDs.
pub async fn get_bots_by_ids(pool: &SqlitePool, ids: &[String]) -> Result<Vec<BotRow>, sqlx::Error> {
    if ids.is_empty() { return Ok(vec![]); }
    let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT id, name, endpoint_url, token_hash, model_family, active, \
         status, submitted_by, description, reviewed_at, reviewed_by, created_at \
         FROM bots WHERE id IN ({}) AND status = 'active'",
        placeholders
    );
    let mut q = sqlx::query_as::<_, BotRow>(&query);
    for id in ids { q = q.bind(id); }
    q.fetch_all(pool).await
}

/// Insert a new debate and return the created row.
pub async fn insert_debate(
    pool: &SqlitePool, id: &str, topic: &str,
) -> Result<DebateRow, sqlx::Error> {
    sqlx::query_as::<_, DebateRow>(
        "INSERT INTO debates (id, topic) VALUES (?, ?) RETURNING *"
    )
    .bind(id).bind(topic).fetch_one(pool).await
}

/// Fetch a single debate by ID, or None if not found.
pub async fn get_debate(pool: &SqlitePool, id: &str) -> Result<Option<DebateRow>, sqlx::Error> {
    sqlx::query_as::<_, DebateRow>("SELECT * FROM debates WHERE id = ?")
        .bind(id).fetch_optional(pool).await
}

/// List debates, optionally filtered by status, most recent first.
pub async fn list_debates(
    pool: &SqlitePool, status: Option<&str>, limit: i64,
) -> Result<Vec<DebateRow>, sqlx::Error> {
    match status {
        Some(s) => {
            sqlx::query_as::<_, DebateRow>(
                "SELECT * FROM debates WHERE status = ? ORDER BY created_at DESC LIMIT ?"
            ).bind(s).bind(limit).fetch_all(pool).await
        }
        None => {
            sqlx::query_as::<_, DebateRow>(
                "SELECT * FROM debates ORDER BY created_at DESC LIMIT ?"
            ).bind(limit).fetch_all(pool).await
        }
    }
}

/// Update a debate's status and set completed_at for terminal states.
pub async fn update_debate_status(
    pool: &SqlitePool, id: &str, status: &str,
) -> Result<(), sqlx::Error> {
    let completed_at = if status == "complete" || status == "failed" || status == "cancelled" {
        Some(chrono::Utc::now().to_rfc3339())
    } else { None };
    sqlx::query("UPDATE debates SET status = ?, completed_at = COALESCE(?, completed_at) WHERE id = ?")
        .bind(status).bind(completed_at).bind(id)
        .execute(pool).await?;
    Ok(())
}

/// Link a bot to a debate with a pseudonym for blind scoring.
pub async fn insert_debate_bot(
    pool: &SqlitePool, debate_id: &str, bot_id: &str, pseudonym: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO debate_bots (debate_id, bot_id, pseudonym) VALUES (?, ?, ?)")
        .bind(debate_id).bind(bot_id).bind(pseudonym)
        .execute(pool).await?;
    Ok(())
}

/// Fetch all bot assignments for a debate.
pub async fn get_debate_bots(
    pool: &SqlitePool, debate_id: &str,
) -> Result<Vec<DebateBotRow>, sqlx::Error> {
    sqlx::query_as::<_, DebateBotRow>("SELECT * FROM debate_bots WHERE debate_id = ?")
        .bind(debate_id).fetch_all(pool).await
}

/// Insert a bot response for a given debate round.
pub async fn insert_response(
    pool: &SqlitePool, id: &str, debate_id: &str, round_number: i64,
    bot_id: &str, response_json: &str, abstained: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO responses (id, debate_id, round_number, bot_id, response_json, abstained) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(id).bind(debate_id).bind(round_number).bind(bot_id).bind(response_json).bind(abstained)
    .execute(pool).await?;
    Ok(())
}

/// Fetch all responses for a specific debate round.
pub async fn get_responses(
    pool: &SqlitePool, debate_id: &str, round_number: i64,
) -> Result<Vec<ResponseRow>, sqlx::Error> {
    sqlx::query_as::<_, ResponseRow>(
        "SELECT * FROM responses WHERE debate_id = ? AND round_number = ?"
    ).bind(debate_id).bind(round_number).fetch_all(pool).await
}

/// Insert a peer score from one bot evaluating another's pseudonym.
pub async fn insert_peer_score(
    pool: &SqlitePool, id: &str, debate_id: &str, scorer_bot_id: &str,
    target_pseudonym: &str, reasoning_quality: i64, factual_grounding: i64,
    overall: i64, reasoning: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO peer_scores (id, debate_id, scorer_bot_id, target_pseudonym, reasoning_quality, factual_grounding, overall, reasoning) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(id).bind(debate_id).bind(scorer_bot_id).bind(target_pseudonym)
    .bind(reasoning_quality).bind(factual_grounding).bind(overall).bind(reasoning)
    .execute(pool).await?;
    Ok(())
}

/// Fetch all peer scores for a debate.
pub async fn get_peer_scores(
    pool: &SqlitePool, debate_id: &str,
) -> Result<Vec<PeerScoreRow>, sqlx::Error> {
    sqlx::query_as::<_, PeerScoreRow>("SELECT * FROM peer_scores WHERE debate_id = ?")
        .bind(debate_id).fetch_all(pool).await
}

/// List bots filtered by status.
pub async fn list_bots_by_status(pool: &SqlitePool, status: &str) -> Result<Vec<BotRow>, sqlx::Error> {
    sqlx::query_as::<_, BotRow>(
        "SELECT id, name, endpoint_url, token_hash, model_family, active, \
         status, submitted_by, description, reviewed_at, reviewed_by, created_at \
         FROM bots WHERE status = ? ORDER BY created_at DESC"
    ).bind(status).fetch_all(pool).await
}

/// List bots submitted by a specific user.
pub async fn list_bots_by_submitter(pool: &SqlitePool, submitted_by: &str) -> Result<Vec<BotRow>, sqlx::Error> {
    sqlx::query_as::<_, BotRow>(
        "SELECT id, name, endpoint_url, token_hash, model_family, active, \
         status, submitted_by, description, reviewed_at, reviewed_by, created_at \
         FROM bots WHERE submitted_by = ? ORDER BY created_at DESC"
    ).bind(submitted_by).fetch_all(pool).await
}

/// List all bots regardless of status (admin use).
pub async fn list_all_bots(pool: &SqlitePool) -> Result<Vec<BotRow>, sqlx::Error> {
    sqlx::query_as::<_, BotRow>(
        "SELECT id, name, endpoint_url, token_hash, model_family, active, \
         status, submitted_by, description, reviewed_at, reviewed_by, created_at \
         FROM bots ORDER BY created_at DESC"
    ).fetch_all(pool).await
}

/// Update bot status (approve, reject, deactivate, reactivate).
pub async fn update_bot_status(
    pool: &SqlitePool, bot_id: &str, new_status: &str, reviewed_by: Option<&str>,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().to_rfc3339();
    let active = matches!(new_status, "active");
    sqlx::query("UPDATE bots SET status = ?, active = ?, reviewed_at = ?, reviewed_by = ? WHERE id = ?")
        .bind(new_status).bind(active).bind(&now).bind(reviewed_by).bind(bot_id)
        .execute(pool).await?;
    Ok(())
}
