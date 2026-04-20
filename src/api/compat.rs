use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::auth::AuthIdentity;
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct LegacyHistoryQuery {
    pub limit: Option<i64>,
}

/// GET /bots/schema — backwards-compatible schema endpoint used by legacy tools.
///
/// This endpoint was removed during API cleanup. We keep this shim so existing
/// automation can discover the expected request/response shape without 404s.
pub async fn legacy_bot_schema() -> AppResult<Json<Value>> {
    Ok(Json(json!({
        "deprecated": true,
        "replacement": "See /bots/guide and /bots/{id}/test for validation flow",
        "request": {
            "type": "object",
            "required": ["session_id", "round", "role", "context", "prompt"],
            "properties": {
                "session_id": { "type": "string" },
                "round": { "type": "integer", "minimum": 0 },
                "role": { "type": "string" },
                "context": { "type": "array" },
                "prompt": { "type": "string" }
            }
        },
        "response": {
            "type": "object",
            "required": ["response"],
            "properties": {
                "response": { "type": "string" },
                "confidence": { "type": "integer", "minimum": 0, "maximum": 100 },
                "challenge": { "type": "object" },
                "position_change": { "type": "object" }
            }
        }
    })))
}

/// GET /bots/{id}/history — backwards-compatible route for legacy diagnostics.
///
/// Returns recent debate summaries as a plain JSON array to match the
/// historical payload shape expected by legacy monitor clients.
pub async fn legacy_bot_history(
    State(state): State<AppState>,
    auth: AuthIdentity,
    Path(id): Path<String>,
    Query(params): Query<LegacyHistoryQuery>,
) -> AppResult<Json<Value>> {
    let bot = queries::get_bot(state.db(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;

    if !auth.is_admin() {
        let Some(user_id) = auth.user_id() else {
            return Err(AppError::Unauthorized);
        };
        match bot.submitted_by.as_deref() {
            Some(owner_id) if owner_id == user_id => {}
            _ => return Err(AppError::Forbidden),
        }
    }

    let limit = params.limit.unwrap_or(25).clamp(1, 200);
    let rows = queries::get_bot_debate_summaries(state.db(), &id, limit).await?;

    let history: Vec<Value> = rows
        .into_iter()
        .map(|row| {
            json!({
                "debate_id": row.debate_id,
                "topic": row.topic,
                "status": row.status,
                "created_at": row.created_at,
                "completed_at": row.completed_at,
                "role": row.role,
                "rounds_total": row.rounds_total,
                "abstained_rounds": row.abstained_rounds,
                "invalid_rounds": row.invalid_rounds,
                "degraded_rounds": row.degraded_rounds
            })
        })
        .collect();

    Ok(Json(Value::Array(history)))
}
