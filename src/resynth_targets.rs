//! Target selection for the resynth CLI (split from resynth.rs for the
//! file-length ceiling).

use crate::db::queries;
use sqlx::SqlitePool;

/// Eligible for automatic rerun: terminal status, not archived, not a
/// non-production topic (the same marker list the cleanup job uses).
///
/// We fetch all debates and filter in code; the list is small enough
/// that issuing the topic filter as SQL isn't worth the string plumbing
/// (queries.rs `list_debates` already handles that for the list endpoint —
/// we don't reuse it because we need ids only).
pub(crate) async fn target_debate_ids(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let rows = queries::list_debates(pool, None, 10_000, false, false).await?;
    Ok(rows
        .into_iter()
        .filter(|r| matches!(r.status.as_str(), "complete" | "failed"))
        .map(|r| r.id)
        .collect())
}
