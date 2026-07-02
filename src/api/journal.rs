//! Admin journal endpoint: the ship's log over HTTP (operator-legibility
//! spec Part 1). Serves the status page and any agent reading the journal
//! directly. (Named `journal` because `api::events` is the SSE stream.)

use crate::api::auth::RequireAdmin;
use crate::error::AppResult;
use crate::observability::events::{SystemEventRow, recent_events};
use crate::state::AppState;
use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;

/// Query parameters for the journal listing.
#[derive(Debug, Deserialize)]
pub struct JournalQuery {
    /// Max entries, newest first. Clamped to 1..=200; default 50.
    pub limit: Option<i64>,
    /// Optional severity filter: info | attention | problem.
    pub severity: Option<String>,
}

/// GET /admin/events — recent journal entries, newest first.
pub async fn list_events(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Query(q): Query<JournalQuery>,
) -> AppResult<Json<Vec<SystemEventRow>>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let rows = recent_events(state.db(), limit, q.severity.as_deref()).await?;
    Ok(Json(rows))
}
