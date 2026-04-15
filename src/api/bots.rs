use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use sha2::{Sha256, Digest};
use crate::api::auth::BearerAuth;
use crate::api::dto::{CreateBotRequest, BotResponse};
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::types::BotId;

/// POST /bots — register a new bot.
pub async fn create_bot(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Json(req): Json<CreateBotRequest>,
) -> AppResult<(StatusCode, Json<BotResponse>)> {
    if req.name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    if req.endpoint_url.is_empty() {
        return Err(AppError::BadRequest("endpoint_url is required".into()));
    }
    if req.token.is_empty() {
        return Err(AppError::BadRequest("token is required".into()));
    }
    let id = BotId::new();
    let token_hash = hex::encode(Sha256::digest(req.token.as_bytes()));
    let row = queries::insert_bot(
        state.db(), id.as_str(), &req.name, &req.endpoint_url, &token_hash,
        req.model_family.as_deref(), None, None, "active",
    ).await?;
    Ok((StatusCode::CREATED, Json(BotResponse {
        id: row.id,
        name: row.name,
        endpoint_url: row.endpoint_url,
        model_family: row.model_family,
        active: row.active,
        created_at: row.created_at,
    })))
}

/// GET /bots — list all active bots.
pub async fn list_bots(
    State(state): State<AppState>,
    _auth: BearerAuth,
) -> AppResult<Json<Vec<BotResponse>>> {
    let rows = queries::list_active_bots(state.db()).await?;
    let bots = rows.into_iter().map(|r| BotResponse {
        id: r.id,
        name: r.name,
        endpoint_url: r.endpoint_url,
        model_family: r.model_family,
        active: r.active,
        created_at: r.created_at,
    }).collect();
    Ok(Json(bots))
}
