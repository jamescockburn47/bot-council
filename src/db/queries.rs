use sqlx::SqlitePool;
use crate::db::models::*;

/// Column list used by every bot SELECT. Kept in one place so schema changes
/// touch one spot instead of six.
const BOT_COLUMNS: &str = "id, name, endpoint_url, token_hash, token_ciphertext, \
    model_family, active, created_at, status, submitted_by, description, \
    rejection_reason, reviewed_at, reviewed_by";

/// Insert a new bot registration and return the created row.
#[allow(clippy::too_many_arguments)]
pub async fn insert_bot(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    endpoint_url: &str,
    token_ciphertext: &[u8],
    model_family: Option<&str>,
    submitted_by: Option<&str>,
    description: Option<&str>,
    status: &str,
) -> Result<BotRow, sqlx::Error> {
    // token_hash = '' satisfies the legacy NOT NULL constraint from the
    // original migration. The column is dropped in a follow-up migration;
    // new code reads token_ciphertext only.
    sqlx::query_as::<_, BotRow>(
        "INSERT INTO bots (id, name, endpoint_url, token_hash, token_ciphertext, \
         model_family, submitted_by, description, status) \
         VALUES (?, ?, ?, '', ?, ?, ?, ?, ?) RETURNING *"
    )
    .bind(id)
    .bind(name)
    .bind(endpoint_url)
    .bind(token_ciphertext)
    .bind(model_family)
    .bind(submitted_by)
    .bind(description)
    .bind(status)
    .fetch_one(pool)
    .await
}

/// Return all active bots ordered by creation time.
pub async fn list_active_bots(pool: &SqlitePool) -> Result<Vec<BotRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {BOT_COLUMNS} FROM bots WHERE status = 'active' ORDER BY created_at"
    );
    sqlx::query_as::<_, BotRow>(&sql).fetch_all(pool).await
}

/// Fetch a single bot by ID, or None if not found.
pub async fn get_bot(pool: &SqlitePool, id: &str) -> Result<Option<BotRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {BOT_COLUMNS} FROM bots WHERE id = ?"
    );
    sqlx::query_as::<_, BotRow>(&sql).bind(id).fetch_optional(pool).await
}

/// Fetch multiple active bots by a slice of IDs.
pub async fn get_bots_by_ids(pool: &SqlitePool, ids: &[String]) -> Result<Vec<BotRow>, sqlx::Error> {
    if ids.is_empty() { return Ok(vec![]); }
    let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT {BOT_COLUMNS} FROM bots WHERE id IN ({placeholders}) AND status = 'active'"
    );
    let mut q = sqlx::query_as::<_, BotRow>(&query);
    for id in ids { q = q.bind(id); }
    q.fetch_all(pool).await
}
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

/// List bots filtered by status.
pub async fn list_bots_by_status(pool: &SqlitePool, status: &str) -> Result<Vec<BotRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {BOT_COLUMNS} FROM bots WHERE status = ? ORDER BY created_at DESC"
    );
    sqlx::query_as::<_, BotRow>(&sql).bind(status).fetch_all(pool).await
}

/// List bots submitted by a specific user.
pub async fn list_bots_by_submitter(pool: &SqlitePool, submitted_by: &str) -> Result<Vec<BotRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {BOT_COLUMNS} FROM bots WHERE submitted_by = ? ORDER BY created_at DESC"
    );
    sqlx::query_as::<_, BotRow>(&sql).bind(submitted_by).fetch_all(pool).await
}

/// List all bots regardless of status (admin use).
pub async fn list_all_bots(pool: &SqlitePool) -> Result<Vec<BotRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {BOT_COLUMNS} FROM bots ORDER BY created_at DESC"
    );
    sqlx::query_as::<_, BotRow>(&sql).fetch_all(pool).await
}

/// Atomically transition a bot's status. Returns the updated row, or `None`
/// if the WHERE clause matched no row (caller then distinguishes "not found"
/// from "wrong state" via get_bot).
pub async fn transition_bot_status(
    pool: &SqlitePool,
    id: &str,
    expected_from: &[&str],
    new_status: &str,
    reviewed_by: Option<&str>,
    rejection_reason: Option<&str>,
) -> Result<Option<BotRow>, sqlx::Error> {
    let placeholders = expected_from.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let active = new_status == "active";
    let sql = format!(
        "UPDATE bots SET status = ?, active = ?, reviewed_at = datetime('now'), \
         reviewed_by = ?, rejection_reason = ? \
         WHERE id = ? AND status IN ({placeholders}) RETURNING *"
    );
    let mut q = sqlx::query_as::<_, BotRow>(&sql)
        .bind(new_status).bind(active)
        .bind(reviewed_by).bind(rejection_reason).bind(id);
    for s in expected_from { q = q.bind(*s); }
    q.fetch_optional(pool).await
}

// ─── Admin registry ────────────────────────────────────────────────────────

/// Admin row returned by list_admins.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct AdminRow {
    pub user_id: String,
    pub granted_at: String,
    pub granted_by: Option<String>,
}

/// Returns true if the given Clerk user_id is in the admins table.
pub async fn is_admin(pool: &SqlitePool, user_id: &str) -> Result<bool, sqlx::Error> {
    let row: Option<(String,)> = sqlx::query_as("SELECT user_id FROM admins WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

/// List all admins, newest grants first.
pub async fn list_admins(pool: &SqlitePool) -> Result<Vec<AdminRow>, sqlx::Error> {
    sqlx::query_as::<_, AdminRow>(
        "SELECT user_id, granted_at, granted_by FROM admins ORDER BY granted_at DESC"
    ).fetch_all(pool).await
}

/// Insert a user_id into the admins table. No-op if already present.
pub async fn add_admin(
    pool: &SqlitePool,
    user_id: &str,
    granted_by: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO admins (user_id, granted_by) VALUES (?, ?) \
         ON CONFLICT(user_id) DO NOTHING"
    )
        .bind(user_id)
        .bind(granted_by)
        .execute(pool)
        .await?;
    Ok(())
}

/// Remove a user_id from the admins table. Returns true if a row was deleted.
pub async fn remove_admin(pool: &SqlitePool, user_id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM admins WHERE user_id = ?")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

// ─── Seen users log ────────────────────────────────────────────────────────

/// Seen user row returned by list_seen_users.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct SeenUserRow {
    pub user_id: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

/// Upsert an entry in the seen_users log. Best-effort — callers should swallow
/// errors rather than fail the authenticated request that triggered the call.
pub async fn upsert_seen_user(pool: &SqlitePool, user_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO seen_users (user_id) VALUES (?) \
         ON CONFLICT(user_id) DO UPDATE SET last_seen_at = datetime('now')"
    )
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// List every user_id that has authenticated at least once, most recent first.
pub async fn list_seen_users(pool: &SqlitePool) -> Result<Vec<SeenUserRow>, sqlx::Error> {
    sqlx::query_as::<_, SeenUserRow>(
        "SELECT user_id, first_seen_at, last_seen_at FROM seen_users ORDER BY last_seen_at DESC"
    ).fetch_all(pool).await
}
