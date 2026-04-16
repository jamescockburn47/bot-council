use axum::extract::{Path, State};
use axum::Json;
use crate::api::auth::BearerAuth;
use crate::api::dto::*;
use crate::db::{queries, queries_phase1};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// GET /debates/{id}/synthesis — final synthesis output (404 if not yet complete).
pub async fn get_synthesis(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Path(id): Path<String>,
) -> AppResult<Json<SynthesisResponse>> {
    let _debate = queries::get_debate(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound(format!("debate {id} not found")))?;

    let synthesis = queries_phase1::get_synthesis(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound(format!("synthesis not yet available for debate {id}")))?;

    let output: serde_json::Value = serde_json::from_str(&synthesis.output_json)
        .unwrap_or_else(|_| serde_json::Value::String(synthesis.output_json.clone()));

    let citation_check: Option<serde_json::Value> = synthesis.citation_check_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());

    Ok(Json(SynthesisResponse {
        debate_id: id,
        synthesis: output,
        model_used: synthesis.model_used,
        created_at: synthesis.created_at,
        citation_check,
    }))
}
