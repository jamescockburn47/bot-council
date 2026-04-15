use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crate::api::auth::BearerAuth;
use crate::api::dto::*;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// POST /debates — create and run a debate (stub).
pub async fn create_debate(
    State(_state): State<AppState>,
    _auth: BearerAuth,
    Json(_req): Json<CreateDebateRequest>,
) -> AppResult<(StatusCode, Json<DebateResponse>)> {
    Err(AppError::Internal(anyhow::anyhow!("not yet implemented")))
}

/// GET /debates — list debates (stub).
pub async fn list_debates(
    State(_state): State<AppState>,
    _auth: BearerAuth,
    Query(_params): Query<ListDebatesQuery>,
) -> AppResult<Json<Vec<DebateResponse>>> {
    Err(AppError::Internal(anyhow::anyhow!("not yet implemented")))
}

/// GET /debates/{id} — get debate detail (stub).
pub async fn get_debate(
    State(_state): State<AppState>,
    _auth: BearerAuth,
    Path(_id): Path<String>,
) -> AppResult<Json<DebateResponse>> {
    Err(AppError::Internal(anyhow::anyhow!("not yet implemented")))
}
