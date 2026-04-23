use crate::db::models::*;
use sqlx::SqlitePool;

/// Insert a round state record.
pub async fn insert_round(
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
    status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO rounds (debate_id, round_number, status) VALUES (?, ?, ?)")
        .bind(debate_id)
        .bind(round_number)
        .bind(status)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get a specific round's state.
pub async fn get_round(
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
) -> Result<Option<RoundRow>, sqlx::Error> {
    sqlx::query_as::<_, RoundRow>("SELECT * FROM rounds WHERE debate_id = ? AND round_number = ?")
        .bind(debate_id)
        .bind(round_number)
        .fetch_optional(pool)
        .await
}

/// Update a round's status. Sets started_at for "in_progress", completed_at for "complete"/"failed".
pub async fn update_round_status(
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
    status: &str,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().to_rfc3339();
    match status {
        "in_progress" => {
            sqlx::query(
                "UPDATE rounds SET status = ?, started_at = ? WHERE debate_id = ? AND round_number = ?"
            )
            .bind(status).bind(&now).bind(debate_id).bind(round_number)
            .execute(pool).await?;
        }
        "complete" | "failed" => {
            sqlx::query(
                "UPDATE rounds SET status = ?, completed_at = ? WHERE debate_id = ? AND round_number = ?"
            )
            .bind(status).bind(&now).bind(debate_id).bind(round_number)
            .execute(pool).await?;
        }
        _ => {
            sqlx::query("UPDATE rounds SET status = ? WHERE debate_id = ? AND round_number = ?")
                .bind(status)
                .bind(debate_id)
                .bind(round_number)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

/// Get all rounds for a debate, ordered by round number.
pub async fn get_rounds(pool: &SqlitePool, debate_id: &str) -> Result<Vec<RoundRow>, sqlx::Error> {
    sqlx::query_as::<_, RoundRow>("SELECT * FROM rounds WHERE debate_id = ? ORDER BY round_number")
        .bind(debate_id)
        .fetch_all(pool)
        .await
}

/// Update a debate_bot's role assignment.
pub async fn update_debate_bot_role(
    pool: &SqlitePool,
    debate_id: &str,
    bot_id: &str,
    role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE debate_bots SET role = ? WHERE debate_id = ? AND bot_id = ?")
        .bind(role)
        .bind(debate_id)
        .bind(bot_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get debate bots with role information.
pub async fn get_debate_bots_with_roles(
    pool: &SqlitePool,
    debate_id: &str,
) -> Result<Vec<DebateBotWithRoleRow>, sqlx::Error> {
    sqlx::query_as::<_, DebateBotWithRoleRow>(
        "SELECT debate_id, bot_id, pseudonym, role FROM debate_bots WHERE debate_id = ?",
    )
    .bind(debate_id)
    .fetch_all(pool)
    .await
}

/// Insert a role history entry for rotation tracking.
pub async fn insert_role_history(
    pool: &SqlitePool,
    bot_id: &str,
    debate_id: &str,
    role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO role_history (bot_id, debate_id, role) VALUES (?, ?, ?)")
        .bind(bot_id)
        .bind(debate_id)
        .bind(role)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get the most recent role for a bot (joins role_history with debates to get the latest by created_at).
pub async fn get_last_role(pool: &SqlitePool, bot_id: &str) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query_as::<_, RoleHistoryRow>(
        "SELECT rh.bot_id, rh.debate_id, rh.role FROM role_history rh \
         JOIN debates d ON rh.debate_id = d.id \
         WHERE rh.bot_id = ? ORDER BY d.created_at DESC LIMIT 1",
    )
    .bind(bot_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.role))
}

/// Insert an analysis result.
pub async fn insert_analysis(
    pool: &SqlitePool,
    id: &str,
    debate_id: &str,
    bot_id: Option<&str>,
    analysis_type: &str,
    input_json: &str,
    result_json: &str,
    model_used: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO analyses (id, debate_id, bot_id, analysis_type, input_json, result_json, model_used) \
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(id).bind(debate_id).bind(bot_id)
    .bind(analysis_type).bind(input_json).bind(result_json).bind(model_used)
    .execute(pool).await?;
    Ok(())
}

/// Get analyses for a debate, optionally filtered by type.
pub async fn get_analyses(
    pool: &SqlitePool,
    debate_id: &str,
    analysis_type: Option<&str>,
) -> Result<Vec<AnalysisRow>, sqlx::Error> {
    match analysis_type {
        Some(t) => sqlx::query_as::<_, AnalysisRow>(
            "SELECT * FROM analyses WHERE debate_id = ? AND analysis_type = ? ORDER BY created_at",
        )
        .bind(debate_id)
        .bind(t)
        .fetch_all(pool)
        .await,
        None => {
            sqlx::query_as::<_, AnalysisRow>(
                "SELECT * FROM analyses WHERE debate_id = ? ORDER BY created_at",
            )
            .bind(debate_id)
            .fetch_all(pool)
            .await
        }
    }
}

/// Insert a cross-examination pairing.
pub async fn insert_pairing(
    pool: &SqlitePool,
    debate_id: &str,
    bot_a_id: &str,
    bot_b_id: &str,
    third_id: Option<&str>,
    pairing_json: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO pairings (debate_id, bot_a_id, bot_b_id, third_id, pairing_json) VALUES (?, ?, ?, ?, ?)"
    )
    .bind(debate_id).bind(bot_a_id).bind(bot_b_id).bind(third_id).bind(pairing_json)
    .execute(pool).await?;
    Ok(())
}

/// Get pairings for a debate.
pub async fn get_pairings(
    pool: &SqlitePool,
    debate_id: &str,
) -> Result<Vec<PairingRow>, sqlx::Error> {
    sqlx::query_as::<_, PairingRow>("SELECT * FROM pairings WHERE debate_id = ?")
        .bind(debate_id)
        .fetch_all(pool)
        .await
}

/// Insert the final synthesis output.
pub async fn insert_synthesis(
    pool: &SqlitePool,
    debate_id: &str,
    output_json: &str,
    model_used: &str,
    prompt_hash: &str,
    citation_check_json: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO syntheses (debate_id, output_json, model_used, prompt_hash, citation_check_json) \
         VALUES (?, ?, ?, ?, ?)"
    )
    .bind(debate_id).bind(output_json).bind(model_used).bind(prompt_hash).bind(citation_check_json)
    .execute(pool).await?;
    Ok(())
}

/// Get the synthesis for a debate.
pub async fn get_synthesis(
    pool: &SqlitePool,
    debate_id: &str,
) -> Result<Option<SynthesisRow>, sqlx::Error> {
    sqlx::query_as::<_, SynthesisRow>("SELECT * FROM syntheses WHERE debate_id = ?")
        .bind(debate_id)
        .fetch_optional(pool)
        .await
}

/// Insert a response with all Phase 1 fields.
#[allow(clippy::too_many_arguments)]
pub async fn insert_response_full(
    pool: &SqlitePool,
    id: &str,
    debate_id: &str,
    round_number: i64,
    bot_id: &str,
    response_json: &str,
    confidence: Option<i64>,
    challenge_json: Option<&str>,
    position_change_json: Option<&str>,
    valid: bool,
    retry_count: i64,
    abstained: bool,
    extraction_metadata: Option<&str>,
    fallback_from_round: Option<i64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO responses \
         (id, debate_id, round_number, bot_id, response_json, confidence, \
          challenge_json, position_change_json, valid, retry_count, abstained, \
          extraction_metadata, fallback_from_round) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(debate_id)
    .bind(round_number)
    .bind(bot_id)
    .bind(response_json)
    .bind(confidence)
    .bind(challenge_json)
    .bind(position_change_json)
    .bind(valid)
    .bind(retry_count)
    .bind(abstained)
    .bind(extraction_metadata)
    .bind(fallback_from_round)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get all responses for a debate across all rounds, ordered by round then creation time.
pub async fn get_all_responses(
    pool: &SqlitePool,
    debate_id: &str,
) -> Result<Vec<ResponseRow>, sqlx::Error> {
    sqlx::query_as::<_, ResponseRow>(
        "SELECT * FROM responses WHERE debate_id = ? ORDER BY round_number, created_at",
    )
    .bind(debate_id)
    .fetch_all(pool)
    .await
}
