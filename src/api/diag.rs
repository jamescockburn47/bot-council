//! Admin-only diagnostic endpoints. Currently exposes `/diag/health`,
//! which is the structured surface Clint polls for alert generation.

use axum::extract::State;
use axum::Json;
use crate::api::auth::RequireAdmin;
use crate::api::dto::DiagHealthResponse;
use crate::error::AppResult;
use crate::state::AppState;

/// GET /diag/health — extended health metrics. Admin only because it
/// surfaces counts (debates in flight, failure rate) that participants
/// shouldn't see.
pub async fn get_diag_health(
    State(state): State<AppState>,
    _admin: RequireAdmin,
) -> AppResult<Json<DiagHealthResponse>> {
    let pool = state.db();

    // Debates currently mid-flight — any status that isn't terminal.
    let in_flight: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM debates \
         WHERE status NOT IN ('complete', 'failed', 'cancelled')",
    )
    .fetch_one(pool)
    .await?;

    // Most recent terminal timestamp.
    let last_completion: (Option<String>,) = sqlx::query_as(
        "SELECT MAX(completed_at) FROM debates \
         WHERE completed_at IS NOT NULL",
    )
    .fetch_one(pool)
    .await?;

    // Failures / terminals in the last hour. `completed_at` is stored
    // as RFC3339 with a 'T' separator; wrap both sides in datetime() so
    // the comparison normalises the format. `COALESCE(SUM, 0)` keeps
    // the first projection an i64 when no rows match.
    let window_counts: (i64, i64) = sqlx::query_as(
        "SELECT \
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0), \
            COUNT(*) \
         FROM debates \
         WHERE completed_at IS NOT NULL \
           AND datetime(completed_at) >= datetime('now', '-1 hour')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or((0, 0));

    let failures_1h = window_counts.0;
    let terminal_1h = window_counts.1;
    let failure_rate_1h = if terminal_1h > 0 {
        Some(failures_1h as f64 / terminal_1h as f64)
    } else {
        None
    };

    let release = std::env::var("SENTRY_RELEASE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into());

    Ok(Json(DiagHealthResponse {
        debates_in_flight: in_flight.0,
        last_completion_ts: last_completion.0,
        failure_rate_1h,
        failures_1h,
        terminal_1h,
        release,
    }))
}
