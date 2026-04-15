use axum::Json;
use axum::extract::State;
use serde_json::{json, Value};
use crate::state::AppState;
use crate::error::AppResult;

/// GET /health — service health + DB connectivity.
pub async fn health(State(state): State<AppState>) -> AppResult<Json<Value>> {
    sqlx::query("SELECT 1").execute(state.db()).await?;
    Ok(Json(json!({ "status": "ok" })))
}
