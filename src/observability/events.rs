//! The ship's log: storage access for `system_events` (operator-legibility
//! spec Part 1). Writes are fire-and-forget — the journal must never break
//! the operation it narrates — and every entry carries both the operator's
//! plain English and the technical handles an agent needs.

use crate::observability::system_guidance;
use sqlx::SqlitePool;

/// What the event is about: a short human label plus optional scoped IDs.
#[derive(Debug, Clone, Copy, Default)]
pub struct EventScope<'a> {
    /// Short human handle used inside the narrative ("debate 1a2b3c4d").
    pub label: &'a str,
    pub debate_id: Option<&'a str>,
    pub bot_id: Option<&'a str>,
}

/// One journal row, as served to the admin events API.
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct SystemEventRow {
    pub id: i64,
    pub created_at: String,
    pub severity: String,
    pub event_kind: String,
    pub narrative: String,
    pub suggested_action: Option<String>,
    pub technical_detail: Option<String>,
    pub debate_id: Option<String>,
    pub bot_id: Option<String>,
}

/// Record a journal entry. Fire-and-forget: on ANY failure this logs at
/// warn and returns — an event-write failure must never fail the operation
/// being recorded.
pub async fn record_event(
    pool: &SqlitePool,
    kind: &str,
    scope: EventScope<'_>,
    detail: &str,
    technical_detail: Option<serde_json::Value>,
) {
    let template = system_guidance::compose(kind, scope.label, detail);
    let technical = technical_detail.map(|v| v.to_string());
    let result = sqlx::query(
        "INSERT INTO system_events \
         (severity, event_kind, narrative, suggested_action, technical_detail, debate_id, bot_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(template.severity)
    .bind(kind)
    .bind(&template.narrative)
    .bind(template.suggested_action)
    .bind(technical)
    .bind(scope.debate_id)
    .bind(scope.bot_id)
    .execute(pool)
    .await;
    if let Err(e) = result {
        tracing::warn!(event_kind = kind, error = %e, "ship's log write failed; operation unaffected");
    }
}

/// Fetch recent journal entries, newest first, optionally filtered by
/// severity. `limit` is clamped by the API layer.
pub async fn recent_events(
    pool: &SqlitePool,
    limit: i64,
    severity: Option<&str>,
) -> Result<Vec<SystemEventRow>, sqlx::Error> {
    match severity {
        Some(s) => {
            sqlx::query_as::<_, SystemEventRow>(
                "SELECT * FROM system_events WHERE severity = ? ORDER BY id DESC LIMIT ?",
            )
            .bind(s)
            .bind(limit)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query_as::<_, SystemEventRow>(
                "SELECT * FROM system_events ORDER BY id DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
}

/// The most recent event of a kind — used at boot to compare the recorded
/// model route against the current one (the lesson-14 drift alarm).
pub async fn last_event_of_kind(
    pool: &SqlitePool,
    kind: &str,
) -> Result<Option<SystemEventRow>, sqlx::Error> {
    sqlx::query_as::<_, SystemEventRow>(
        "SELECT * FROM system_events WHERE event_kind = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(kind)
    .fetch_one(pool)
    .await
    .map(Some)
    .or_else(|e| match e {
        sqlx::Error::RowNotFound => Ok(None),
        other => Err(other),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!().run(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn record_and_fetch_round_trip() {
        let pool = pool().await;
        record_event(
            &pool,
            "debate_failed",
            EventScope {
                label: "debate 1a2b3c4d",
                debate_id: Some("1a2b3c4d"),
                bot_id: None,
            },
            "The summariser was unreachable.",
            Some(serde_json::json!({"error": "connect timeout"})),
        )
        .await;
        let rows = recent_events(&pool, 10, None).await.expect("fetch");
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.event_kind, "debate_failed");
        assert_eq!(row.severity, "problem");
        assert!(row.narrative.contains("could not finish"));
        assert!(row.suggested_action.is_some());
        assert_eq!(row.debate_id.as_deref(), Some("1a2b3c4d"));
        assert!(
            row.technical_detail
                .as_deref()
                .is_some_and(|t| t.contains("connect timeout"))
        );
    }

    #[tokio::test]
    async fn unknown_kind_still_records() {
        let pool = pool().await;
        record_event(&pool, "mystery_kind", EventScope::default(), "", None).await;
        let rows = recent_events(&pool, 10, None).await.expect("fetch");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].severity, "attention");
    }

    #[tokio::test]
    async fn write_failure_is_swallowed() {
        let pool = pool().await;
        pool.close().await;
        // Must not panic or propagate.
        record_event(&pool, "service_started", EventScope::default(), "", None).await;
    }

    #[tokio::test]
    async fn severity_filter_and_last_of_kind() {
        let pool = pool().await;
        record_event(&pool, "service_started", EventScope::default(), "", None).await;
        record_event(
            &pool,
            "quorum_not_met",
            EventScope {
                label: "debate x",
                debate_id: Some("x"),
                bot_id: None,
            },
            "",
            None,
        )
        .await;
        let problems = recent_events(&pool, 10, Some("problem")).await.expect("fetch");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].event_kind, "quorum_not_met");
        let last = last_event_of_kind(&pool, "service_started")
            .await
            .expect("query");
        assert!(last.is_some());
        assert!(
            last_event_of_kind(&pool, "resynth_run")
                .await
                .expect("query")
                .is_none()
        );
    }
}
