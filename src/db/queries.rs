use crate::db::models::*;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Column list used by every bot SELECT. Kept in one place so schema changes
/// touch one spot instead of six.
const BOT_COLUMNS: &str = "id, name, endpoint_url, token_ciphertext, \
    model_family, created_at, status, submitted_by, description, \
    rejection_reason, reviewed_at, reviewed_by, bot_kind, introduction";

/// Marker phrases used to exclude operator/test debates from public listings,
/// bot performance scoring, and to drive the scheduled test-debate cleanup.
pub(crate) const NON_PRODUCTION_TOPIC_MARKERS: &[&str] = &[
    "quickfire readiness check",
    "candidate test",
    "smoke test",
    "smoke run",
    "probe run",
    "verification run",
    "synthesis path verification",
    "local synthesis",
    "gemma synthesis verification",
    "token compatibility regression check",
    "simplified-route",
    "what is the meaning of life?",
    "strict time budgets",
];

fn non_production_topic_filter(topic_expr: &str) -> String {
    NON_PRODUCTION_TOPIC_MARKERS
        .iter()
        .map(|marker| format!("instr(lower({topic_expr}), '{marker}') = 0"))
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn only_non_production_topic_filter(topic_expr: &str) -> String {
    NON_PRODUCTION_TOPIC_MARKERS
        .iter()
        .map(|marker| format!("instr(lower({topic_expr}), '{marker}') > 0"))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Insert a new bot registration and return the created row.
#[allow(clippy::too_many_arguments)]
pub async fn insert_bot(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    endpoint_url: &str,
    token_ciphertext: Option<&[u8]>,
    model_family: Option<&str>,
    submitted_by: Option<&str>,
    description: Option<&str>,
    status: &str,
) -> Result<BotRow, sqlx::Error> {
    sqlx::query_as::<_, BotRow>(
        "INSERT INTO bots (id, name, endpoint_url, token_ciphertext, \
         model_family, submitted_by, description, status) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
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
    let sql = format!("SELECT {BOT_COLUMNS} FROM bots WHERE status = 'active' ORDER BY created_at");
    sqlx::query_as::<_, BotRow>(&sql).fetch_all(pool).await
}

/// Fetch a single bot by ID, or None if not found.
pub async fn get_bot(pool: &SqlitePool, id: &str) -> Result<Option<BotRow>, sqlx::Error> {
    let sql = format!("SELECT {BOT_COLUMNS} FROM bots WHERE id = ?");
    sqlx::query_as::<_, BotRow>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Fetch multiple active bots by a slice of IDs.
pub async fn get_bots_by_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<BotRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT {BOT_COLUMNS} FROM bots WHERE id IN ({placeholders}) AND status = 'active'"
    );
    let mut q = sqlx::query_as::<_, BotRow>(&query);
    for id in ids {
        q = q.bind(id);
    }
    q.fetch_all(pool).await
}

/// Fetch bots by IDs regardless of status.
pub async fn get_any_bots_by_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<BotRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!("SELECT {BOT_COLUMNS} FROM bots WHERE id IN ({placeholders})");
    let mut q = sqlx::query_as::<_, BotRow>(&query);
    for id in ids {
        q = q.bind(id);
    }
    q.fetch_all(pool).await
}
pub async fn insert_debate(
    pool: &SqlitePool,
    id: &str,
    topic: &str,
) -> Result<DebateRow, sqlx::Error> {
    sqlx::query_as::<_, DebateRow>("INSERT INTO debates (id, topic) VALUES (?, ?) RETURNING *")
        .bind(id)
        .bind(topic)
        .fetch_one(pool)
        .await
}

/// Fetch a single debate by ID, or None if not found.
pub async fn get_debate(pool: &SqlitePool, id: &str) -> Result<Option<DebateRow>, sqlx::Error> {
    sqlx::query_as::<_, DebateRow>("SELECT * FROM debates WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// List debates, optionally filtered by status, most recent first.
/// When `include_archived` is false (the default), soft-deleted debates
/// are hidden. Admins can pass `?archived=true` to see them.
pub async fn list_debates(
    pool: &SqlitePool,
    status: Option<&str>,
    limit: i64,
    test_only: bool,
    include_archived: bool,
) -> Result<Vec<DebateRow>, sqlx::Error> {
    let topic_filter = if test_only {
        only_non_production_topic_filter("topic")
    } else {
        non_production_topic_filter("topic")
    };
    let archived_filter = if include_archived {
        ""
    } else {
        " AND archived_at IS NULL"
    };
    match status {
        Some(s) => {
            let sql = format!(
                "SELECT * FROM debates WHERE status = ? AND ({topic_filter}){archived_filter} ORDER BY created_at DESC LIMIT ?"
            );
            sqlx::query_as::<_, DebateRow>(&sql)
                .bind(s)
                .bind(limit)
                .fetch_all(pool)
                .await
        }
        None => {
            let sql = format!(
                "SELECT * FROM debates WHERE ({topic_filter}){archived_filter} ORDER BY created_at DESC LIMIT ?"
            );
            sqlx::query_as::<_, DebateRow>(&sql)
                .bind(limit)
                .fetch_all(pool)
                .await
        }
    }
}

/// Set or clear a debate's archived_at timestamp.
pub async fn set_debate_archived(
    pool: &SqlitePool,
    id: &str,
    archived: bool,
) -> Result<(), sqlx::Error> {
    let value = if archived {
        Some(chrono::Utc::now().to_rfc3339())
    } else {
        None
    };
    sqlx::query("UPDATE debates SET archived_at = ? WHERE id = ?")
        .bind(value)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update a debate's status and set completed_at for terminal states.
pub async fn update_debate_status(
    pool: &SqlitePool,
    id: &str,
    status: &str,
) -> Result<(), sqlx::Error> {
    let completed_at = if status == "complete" || status == "failed" || status == "cancelled" {
        Some(chrono::Utc::now().to_rfc3339())
    } else {
        None
    };
    sqlx::query(
        "UPDATE debates SET status = ?, completed_at = COALESCE(?, completed_at) WHERE id = ?",
    )
    .bind(status)
    .bind(completed_at)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Link a bot to a debate with a pseudonym for blind scoring.
pub async fn insert_debate_bot(
    pool: &SqlitePool,
    debate_id: &str,
    bot_id: &str,
    pseudonym: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO debate_bots (debate_id, bot_id, pseudonym) VALUES (?, ?, ?)")
        .bind(debate_id)
        .bind(bot_id)
        .bind(pseudonym)
        .execute(pool)
        .await?;
    Ok(())
}

/// Fetch all bot assignments for a debate.
pub async fn get_debate_bots(
    pool: &SqlitePool,
    debate_id: &str,
) -> Result<Vec<DebateBotRow>, sqlx::Error> {
    sqlx::query_as::<_, DebateBotRow>("SELECT * FROM debate_bots WHERE debate_id = ?")
        .bind(debate_id)
        .fetch_all(pool)
        .await
}

/// Insert a bot response for a given debate round.
pub async fn insert_response(
    pool: &SqlitePool,
    id: &str,
    debate_id: &str,
    round_number: i64,
    bot_id: &str,
    response_json: &str,
    abstained: bool,
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
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
) -> Result<Vec<ResponseRow>, sqlx::Error> {
    sqlx::query_as::<_, ResponseRow>(
        "SELECT * FROM responses WHERE debate_id = ? AND round_number = ?",
    )
    .bind(debate_id)
    .bind(round_number)
    .fetch_all(pool)
    .await
}

/// Insert a peer score from one bot evaluating another's pseudonym.
pub async fn insert_peer_score(
    pool: &SqlitePool,
    id: &str,
    debate_id: &str,
    scorer_bot_id: &str,
    target_pseudonym: &str,
    reasoning_quality: i64,
    factual_grounding: i64,
    overall: i64,
    reasoning: &str,
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
    pool: &SqlitePool,
    debate_id: &str,
) -> Result<Vec<PeerScoreRow>, sqlx::Error> {
    sqlx::query_as::<_, PeerScoreRow>("SELECT * FROM peer_scores WHERE debate_id = ?")
        .bind(debate_id)
        .fetch_all(pool)
        .await
}

/// List bots filtered by status.
pub async fn list_bots_by_status(
    pool: &SqlitePool,
    status: &str,
) -> Result<Vec<BotRow>, sqlx::Error> {
    let sql = format!("SELECT {BOT_COLUMNS} FROM bots WHERE status = ? ORDER BY created_at DESC");
    sqlx::query_as::<_, BotRow>(&sql)
        .bind(status)
        .fetch_all(pool)
        .await
}

/// List bots submitted by a specific user.
pub async fn list_bots_by_submitter(
    pool: &SqlitePool,
    submitted_by: &str,
) -> Result<Vec<BotRow>, sqlx::Error> {
    let sql =
        format!("SELECT {BOT_COLUMNS} FROM bots WHERE submitted_by = ? ORDER BY created_at DESC");
    sqlx::query_as::<_, BotRow>(&sql)
        .bind(submitted_by)
        .fetch_all(pool)
        .await
}

/// Archive all prior submissions from the same submitter and bot name.
///
/// This keeps one canonical "latest" submission per owner+name while preserving
/// historical rows for audit/debug.
pub async fn archive_prior_submissions_for_submitter(
    pool: &SqlitePool,
    submitted_by: &str,
    name: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE bots SET status = 'inactive' \
         WHERE submitted_by = ? \
           AND lower(trim(name)) = lower(trim(?)) \
           AND status IN ('active', 'pending', 'smoke_test_failed', 'rejected')",
    )
    .bind(submitted_by)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// List all bots regardless of status (admin use).
pub async fn list_all_bots(pool: &SqlitePool) -> Result<Vec<BotRow>, sqlx::Error> {
    let sql = format!("SELECT {BOT_COLUMNS} FROM bots ORDER BY created_at DESC");
    sqlx::query_as::<_, BotRow>(&sql).fetch_all(pool).await
}

/// Aggregate per-bot debate performance metrics across stored responses.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BotPerformanceAggregate {
    pub bot_id: String,
    pub total_rounds: i64,
    pub debates_participated: i64,
    pub abstained_rounds: i64,
    pub invalid_rounds: i64,
    pub degraded_rounds: i64,
    pub last_debate_at: Option<String>,
}

/// Snapshot payload persisted by weekly scoreboard runs.
#[derive(Debug, Clone)]
pub struct BotScoreSnapshotInput {
    pub bot_id: String,
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
    pub suggestions_json: String,
}

/// Raw response samples used to derive quality-oriented scoring dimensions.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BotResponseSampleRow {
    pub bot_id: String,
    pub response_json: String,
    pub abstained: bool,
    pub valid: bool,
}

/// Fetch performance aggregates keyed by bot id.
pub async fn get_bot_performance_aggregates(
    pool: &SqlitePool,
    bot_ids: &[String],
) -> Result<HashMap<String, BotPerformanceAggregate>, sqlx::Error> {
    if bot_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = bot_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let topic_filter = non_production_topic_filter("d.topic");
    let sql = format!(
        "SELECT
            r.bot_id AS bot_id,
            COUNT(*) AS total_rounds,
            COUNT(DISTINCT r.debate_id) AS debates_participated,
            COALESCE(SUM(CASE WHEN r.abstained = 1 THEN 1 ELSE 0 END), 0) AS abstained_rounds,
            COALESCE(SUM(CASE WHEN r.valid = 0 THEN 1 ELSE 0 END), 0) AS invalid_rounds,
            COALESCE(SUM(CASE
                WHEN instr(lower(r.response_json), 'unable to formulate') > 0 THEN 1
                WHEN instr(lower(r.response_json), 'unable to provide') > 0 THEN 1
                ELSE 0
            END), 0) AS degraded_rounds,
            MAX(d.created_at) AS last_debate_at
         FROM responses r
         JOIN debates d ON d.id = r.debate_id
         WHERE r.bot_id IN ({placeholders})
           AND {topic_filter}
         GROUP BY r.bot_id"
    );
    let mut q = sqlx::query_as::<_, BotPerformanceAggregate>(&sql);
    for id in bot_ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.bot_id.clone(), row))
        .collect())
}

/// Fetch recent response samples keyed by bot id.
pub async fn get_bot_response_samples(
    pool: &SqlitePool,
    bot_ids: &[String],
    per_bot_limit: i64,
) -> Result<HashMap<String, Vec<BotResponseSampleRow>>, sqlx::Error> {
    if bot_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = bot_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let topic_filter = non_production_topic_filter("d.topic");
    let sql = format!(
        "SELECT bot_id, response_json, abstained, valid
         FROM (
            SELECT
                r.bot_id AS bot_id,
                r.response_json AS response_json,
                r.abstained AS abstained,
                r.valid AS valid,
                ROW_NUMBER() OVER (PARTITION BY r.bot_id ORDER BY r.created_at DESC) AS row_num
            FROM responses r
            JOIN debates d ON d.id = r.debate_id
            WHERE r.bot_id IN ({placeholders})
              AND {topic_filter}
         ) ranked
         WHERE row_num <= ?"
    );
    let mut q = sqlx::query_as::<_, BotResponseSampleRow>(&sql);
    for id in bot_ids {
        q = q.bind(id);
    }
    q = q.bind(per_bot_limit);

    let rows = q.fetch_all(pool).await?;
    let mut by_bot: HashMap<String, Vec<BotResponseSampleRow>> = HashMap::new();
    for row in rows {
        by_bot.entry(row.bot_id.clone()).or_default().push(row);
    }
    Ok(by_bot)
}

/// Debate-level analytics for a single bot.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct BotDebateSummaryRow {
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

/// Return recent debate summaries for a bot.
pub async fn get_bot_debate_summaries(
    pool: &SqlitePool,
    bot_id: &str,
    limit: i64,
) -> Result<Vec<BotDebateSummaryRow>, sqlx::Error> {
    let topic_filter = non_production_topic_filter("d.topic");
    let sql = format!(
        "SELECT
            d.id AS debate_id,
            d.topic AS topic,
            d.status AS status,
            d.created_at AS created_at,
            d.completed_at AS completed_at,
            db.role AS role,
            COUNT(r.id) AS rounds_total,
            COALESCE(SUM(CASE WHEN r.abstained = 1 THEN 1 ELSE 0 END), 0) AS abstained_rounds,
            COALESCE(SUM(CASE WHEN r.valid = 0 THEN 1 ELSE 0 END), 0) AS invalid_rounds,
            COALESCE(SUM(CASE
                WHEN instr(lower(r.response_json), 'unable to formulate') > 0 THEN 1
                WHEN instr(lower(r.response_json), 'unable to provide') > 0 THEN 1
                ELSE 0
            END), 0) AS degraded_rounds
         FROM debate_bots db
         JOIN debates d ON d.id = db.debate_id
         LEFT JOIN responses r
             ON r.debate_id = db.debate_id
            AND r.bot_id = db.bot_id
         WHERE db.bot_id = ?
           AND {topic_filter}
         GROUP BY d.id, d.topic, d.status, d.created_at, d.completed_at, db.role
         ORDER BY d.created_at DESC
         LIMIT ?"
    );
    sqlx::query_as::<_, BotDebateSummaryRow>(&sql)
        .bind(bot_id)
        .bind(limit)
        .fetch_all(pool)
        .await
}

/// Upsert a weekly scoreboard snapshot for each bot.
pub async fn upsert_bot_score_snapshots(
    pool: &SqlitePool,
    snapshot_week: &str,
    model_used: &str,
    snapshots: &[BotScoreSnapshotInput],
) -> Result<(), sqlx::Error> {
    if snapshots.is_empty() {
        return Ok(());
    }
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS bot_score_snapshots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            bot_id TEXT NOT NULL,
            snapshot_week TEXT NOT NULL,
            computed_at TEXT NOT NULL DEFAULT (datetime('now')),
            model_used TEXT NOT NULL,
            score_out_of_10 REAL NOT NULL,
            critical_thinking_score_out_of_10 REAL NOT NULL,
            resource_use_score_out_of_10 REAL NOT NULL,
            instruction_following_score_out_of_10 REAL NOT NULL,
            functionality_score_out_of_10 REAL NOT NULL,
            usefulness_score_out_of_10 REAL NOT NULL,
            debate_engagement_score_out_of_10 REAL NOT NULL,
            total_rounds INTEGER NOT NULL,
            debates_participated INTEGER NOT NULL,
            abstained_rounds INTEGER NOT NULL,
            invalid_rounds INTEGER NOT NULL,
            degraded_rounds INTEGER NOT NULL,
            last_debate_at TEXT,
            suggestions_json TEXT NOT NULL,
            FOREIGN KEY (bot_id) REFERENCES bots(id) ON DELETE CASCADE,
            UNIQUE (bot_id, snapshot_week)
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_bot_score_snapshots_bot_week
         ON bot_score_snapshots(bot_id, snapshot_week DESC)",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_bot_score_snapshots_week
         ON bot_score_snapshots(snapshot_week DESC)",
    )
    .execute(pool)
    .await?;
    let mut tx = pool.begin().await?;
    for snapshot in snapshots {
        sqlx::query(
            "INSERT INTO bot_score_snapshots (
                bot_id,
                snapshot_week,
                model_used,
                score_out_of_10,
                critical_thinking_score_out_of_10,
                resource_use_score_out_of_10,
                instruction_following_score_out_of_10,
                functionality_score_out_of_10,
                usefulness_score_out_of_10,
                debate_engagement_score_out_of_10,
                total_rounds,
                debates_participated,
                abstained_rounds,
                invalid_rounds,
                degraded_rounds,
                last_debate_at,
                suggestions_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(bot_id, snapshot_week) DO UPDATE SET
                computed_at = datetime('now'),
                model_used = excluded.model_used,
                score_out_of_10 = excluded.score_out_of_10,
                critical_thinking_score_out_of_10 = excluded.critical_thinking_score_out_of_10,
                resource_use_score_out_of_10 = excluded.resource_use_score_out_of_10,
                instruction_following_score_out_of_10 = excluded.instruction_following_score_out_of_10,
                functionality_score_out_of_10 = excluded.functionality_score_out_of_10,
                usefulness_score_out_of_10 = excluded.usefulness_score_out_of_10,
                debate_engagement_score_out_of_10 = excluded.debate_engagement_score_out_of_10,
                total_rounds = excluded.total_rounds,
                debates_participated = excluded.debates_participated,
                abstained_rounds = excluded.abstained_rounds,
                invalid_rounds = excluded.invalid_rounds,
                degraded_rounds = excluded.degraded_rounds,
                last_debate_at = excluded.last_debate_at,
                suggestions_json = excluded.suggestions_json"
        )
        .bind(&snapshot.bot_id)
        .bind(snapshot_week)
        .bind(model_used)
        .bind(snapshot.score_out_of_10)
        .bind(snapshot.critical_thinking_score_out_of_10)
        .bind(snapshot.resource_use_score_out_of_10)
        .bind(snapshot.instruction_following_score_out_of_10)
        .bind(snapshot.functionality_score_out_of_10)
        .bind(snapshot.usefulness_score_out_of_10)
        .bind(snapshot.debate_engagement_score_out_of_10)
        .bind(snapshot.total_rounds)
        .bind(snapshot.debates_participated)
        .bind(snapshot.abstained_rounds)
        .bind(snapshot.invalid_rounds)
        .bind(snapshot.degraded_rounds)
        .bind(&snapshot.last_debate_at)
        .bind(&snapshot.suggestions_json)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
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
    let placeholders = expected_from
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "UPDATE bots SET status = ?, reviewed_at = datetime('now'), \
         reviewed_by = ?, rejection_reason = ? \
         WHERE id = ? AND status IN ({placeholders}) RETURNING *"
    );
    let mut q = sqlx::query_as::<_, BotRow>(&sql)
        .bind(new_status)
        .bind(reviewed_by)
        .bind(rejection_reason)
        .bind(id);
    for s in expected_from {
        q = q.bind(*s);
    }
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
        "SELECT user_id, granted_at, granted_by FROM admins ORDER BY granted_at DESC",
    )
    .fetch_all(pool)
    .await
}

/// Insert a user_id into the admins table. No-op if already present.
pub async fn add_admin(
    pool: &SqlitePool,
    user_id: &str,
    granted_by: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO admins (user_id, granted_by) VALUES (?, ?) \
         ON CONFLICT(user_id) DO NOTHING",
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
         ON CONFLICT(user_id) DO UPDATE SET last_seen_at = datetime('now')",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// List every user_id that has authenticated at least once, most recent first.
pub async fn list_seen_users(pool: &SqlitePool) -> Result<Vec<SeenUserRow>, sqlx::Error> {
    sqlx::query_as::<_, SeenUserRow>(
        "SELECT user_id, first_seen_at, last_seen_at FROM seen_users ORDER BY last_seen_at DESC",
    )
    .fetch_all(pool)
    .await
}
