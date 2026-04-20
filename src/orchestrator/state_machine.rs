use crate::db::queries_phase1;
use sqlx::SqlitePool;

/// Initialise round records for a new multi-round debate.
pub async fn init_rounds(
    pool: &SqlitePool,
    debate_id: &str,
    total_rounds: i64,
) -> Result<(), String> {
    for round in 0..total_rounds {
        queries_phase1::insert_round(pool, debate_id, round, "pending")
            .await
            .map_err(|e| format!("failed to init round {round}: {e}"))?;
    }
    Ok(())
}

/// Mark a round as in-progress.
pub async fn start_round(
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
) -> Result<(), String> {
    queries_phase1::update_round_status(pool, debate_id, round_number, "in_progress")
        .await
        .map_err(|e| format!("failed to start round {round_number}: {e}"))
}

/// Mark a round as complete.
pub async fn complete_round(
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
) -> Result<(), String> {
    queries_phase1::update_round_status(pool, debate_id, round_number, "complete")
        .await
        .map_err(|e| format!("failed to complete round {round_number}: {e}"))
}

/// Mark a round as failed.
pub async fn fail_round(
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
) -> Result<(), String> {
    queries_phase1::update_round_status(pool, debate_id, round_number, "failed")
        .await
        .map_err(|e| format!("failed to fail round {round_number}: {e}"))
}

/// Find the next round to run for resumption. Returns the first round
/// that is not yet complete. Returns None if all rounds are complete.
pub async fn find_resume_point(pool: &SqlitePool, debate_id: &str) -> Result<Option<i64>, String> {
    let rounds = queries_phase1::get_rounds(pool, debate_id)
        .await
        .map_err(|e| format!("failed to get rounds: {e}"))?;

    for round in rounds {
        if round.status != "complete" {
            return Ok(Some(round.round_number));
        }
    }
    Ok(None)
}
