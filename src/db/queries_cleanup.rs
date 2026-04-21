//! Background-job queries: find & cascade-delete stale test debates.
//!
//! Used only by the scheduled `test-cleanup` job fired from
//! `deploy/bot-council-test-cleanup.timer`. The admin-facing delete
//! (Phase D3) will reuse `cascade_delete_debate` but add its own
//! authorisation + audit-log layer — keep this file free of policy.

use crate::db::queries::NON_PRODUCTION_TOPIC_MARKERS;
use sqlx::SqlitePool;

/// Tables whose rows reference `debates(id)` without `ON DELETE CASCADE`.
/// SQLite can't add CASCADE to an existing FK without rebuilding the table,
/// so we cascade manually in a transaction before deleting the debate row.
const CHILD_TABLES: &[&str] = &[
    "analyses",
    "debate_bots",
    "pairings",
    "peer_scores",
    "responses",
    "role_history",
    "rounds",
    "syntheses",
];

/// Build a WHERE fragment matching any of `NON_PRODUCTION_TOPIC_MARKERS`
/// against `lower(topic)`. Marker strings are treated as literals and any
/// embedded `'` is escaped by doubling.
fn test_topic_match_sql() -> String {
    NON_PRODUCTION_TOPIC_MARKERS
        .iter()
        .map(|m| format!("instr(lower(topic), '{}') > 0", m.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(" OR ")
}

/// Find debate IDs eligible for test-cleanup: terminal status, topic matches
/// a non-production marker, and older than `grace_hours` so an operator
/// still reading a just-ran probe is protected.
pub async fn find_stale_test_debate_ids(
    pool: &SqlitePool,
    grace_hours: i64,
) -> Result<Vec<String>, sqlx::Error> {
    let topic_match = test_topic_match_sql();
    let sql = format!(
        "SELECT id FROM debates \
         WHERE status IN ('complete', 'cancelled', 'failed') \
         AND ({topic_match}) \
         AND created_at < datetime('now', ?) \
         ORDER BY created_at ASC"
    );
    let rows: Vec<(String,)> = sqlx::query_as(&sql)
        .bind(format!("-{grace_hours} hours"))
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Delete a debate and every child row that references it, in one
/// transaction. Idempotent: re-running against an already-deleted id
/// succeeds as a no-op (no rows affected, no error).
pub async fn cascade_delete_debate(pool: &SqlitePool, debate_id: &str) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for table in CHILD_TABLES {
        let sql = format!("DELETE FROM {table} WHERE debate_id = ?");
        sqlx::query(&sql).bind(debate_id).execute(&mut *tx).await?;
    }
    sqlx::query("DELETE FROM debates WHERE id = ?")
        .bind(debate_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn setup() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn insert_debate(pool: &SqlitePool, id: &str, topic: &str, status: &str, hours_ago: i64) {
        sqlx::query(
            "INSERT INTO debates (id, topic, status, created_at) \
             VALUES (?, ?, ?, datetime('now', ?))",
        )
        .bind(id)
        .bind(topic)
        .bind(status)
        .bind(format!("-{hours_ago} hours"))
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn finds_only_terminal_old_test_debates() {
        let pool = setup().await;
        insert_debate(&pool, "old-test-complete", "smoke test foo", "complete", 2).await;
        insert_debate(&pool, "old-test-running", "smoke test foo", "round_1", 2).await;
        insert_debate(&pool, "new-test-complete", "smoke test foo", "complete", 0).await;
        insert_debate(
            &pool,
            "old-prod-complete",
            "Will AI replace judges?",
            "complete",
            2,
        )
        .await;
        insert_debate(
            &pool,
            "old-test-failed",
            "verification run bar",
            "failed",
            2,
        )
        .await;

        let ids = find_stale_test_debate_ids(&pool, 1).await.unwrap();
        // old-test-complete + old-test-failed: terminal, test topic, beyond grace
        // old-test-running: not terminal → excluded
        // new-test-complete: within grace → excluded
        // old-prod-complete: non-test topic → excluded
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"old-test-complete".to_string()));
        assert!(ids.contains(&"old-test-failed".to_string()));
    }

    #[tokio::test]
    async fn cascade_delete_removes_debate_and_child_rows() {
        let pool = setup().await;
        insert_debate(&pool, "d1", "smoke test cascade", "complete", 2).await;
        // A row in `rounds` is enough to prove cascade — its FK is only
        // `debates(id)`, unlike `debate_bots` which also requires a matching
        // `bots(id)` row we'd have to synthesise.
        sqlx::query(
            "INSERT INTO rounds (debate_id, round_number, status) VALUES ('d1', 0, 'complete')",
        )
        .execute(&pool)
        .await
        .unwrap();

        cascade_delete_debate(&pool, "d1").await.unwrap();

        let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM debates WHERE id = 'd1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(remaining, 0);
        let child: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rounds WHERE debate_id = 'd1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(child, 0);
    }

    #[tokio::test]
    async fn cascade_delete_is_idempotent_for_missing_id() {
        let pool = setup().await;
        // Should not error — delete of 0 rows is a no-op.
        cascade_delete_debate(&pool, "never-existed").await.unwrap();
    }
}
