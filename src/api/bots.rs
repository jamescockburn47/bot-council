use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use sha2::{Sha256, Digest};
use crate::api::auth::{AuthIdentity, AdminOnly};
use crate::api::dto::{CreateBotRequest, BotResponse, UserInfoResponse};
use crate::db::{models::BotRow, queries};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::types::BotId;

/// Convert a database row to an API response.
fn bot_to_response(row: &BotRow) -> BotResponse {
    BotResponse {
        id: row.id.clone(),
        name: row.name.clone(),
        endpoint_url: row.endpoint_url.clone(),
        model_family: row.model_family.clone(),
        active: row.active,
        status: row.status.clone(),
        description: row.description.clone(),
        submitted_by: row.submitted_by.clone(),
        reviewed_at: row.reviewed_at.clone(),
        reviewed_by: row.reviewed_by.clone(),
        created_at: row.created_at.clone(),
    }
}

/// POST /bots — register a new bot.
///
/// Members create bots as pending; admins create as active.
pub async fn create_bot(
    State(state): State<AppState>,
    auth: AuthIdentity,
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
    let status = if auth.is_admin() { "active" } else { "pending" };
    let submitted_by = auth.user_id().map(String::from);
    let row = queries::insert_bot(
        state.db(), id.as_str(), &req.name, &req.endpoint_url, &token_hash,
        req.model_family.as_deref(), submitted_by.as_deref(),
        req.description.as_deref(), status,
    ).await?;
    Ok((StatusCode::CREATED, Json(bot_to_response(&row))))
}

/// GET /bots — list bots.
///
/// Admins see all bots; members see only active.
pub async fn list_bots(
    State(state): State<AppState>,
    auth: AuthIdentity,
) -> AppResult<Json<Vec<BotResponse>>> {
    let rows = if auth.is_admin() {
        queries::list_all_bots(state.db()).await?
    } else {
        queries::list_active_bots(state.db()).await?
    };
    let bots = rows.iter().map(bot_to_response).collect();
    Ok(Json(bots))
}

/// PATCH /bots/{id}/approve — approve a pending bot (admin only).
pub async fn approve_bot(
    State(state): State<AppState>,
    admin: AdminOnly,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let bot = queries::get_bot(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    if bot.status != "pending" {
        return Err(AppError::BadRequest("bot is not pending".into()));
    }
    queries::update_bot_status(
        state.db(), &id, "active", admin.0.user_id(),
    ).await?;
    let updated = queries::get_bot(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    Ok(Json(bot_to_response(&updated)))
}

/// PATCH /bots/{id}/reject — reject a pending bot (admin only).
pub async fn reject_bot(
    State(state): State<AppState>,
    admin: AdminOnly,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let bot = queries::get_bot(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    if bot.status != "pending" {
        return Err(AppError::BadRequest("bot is not pending".into()));
    }
    queries::update_bot_status(
        state.db(), &id, "rejected", admin.0.user_id(),
    ).await?;
    let updated = queries::get_bot(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    Ok(Json(bot_to_response(&updated)))
}

/// PATCH /bots/{id}/deactivate — deactivate an active bot (admin only).
pub async fn deactivate_bot(
    State(state): State<AppState>,
    admin: AdminOnly,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let bot = queries::get_bot(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    if bot.status != "active" {
        return Err(AppError::BadRequest("bot is not active".into()));
    }
    queries::update_bot_status(
        state.db(), &id, "inactive", admin.0.user_id(),
    ).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// PATCH /bots/{id}/reactivate — reactivate an inactive bot (admin only).
pub async fn reactivate_bot(
    State(state): State<AppState>,
    admin: AdminOnly,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let bot = queries::get_bot(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    if bot.status != "inactive" {
        return Err(AppError::BadRequest("bot is not inactive".into()));
    }
    queries::update_bot_status(
        state.db(), &id, "active", admin.0.user_id(),
    ).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /bots/my-submissions — list bots submitted by the current user.
pub async fn my_submissions(
    State(state): State<AppState>,
    auth: AuthIdentity,
) -> AppResult<Json<Vec<BotResponse>>> {
    let user_id = auth.user_id()
        .ok_or_else(|| AppError::BadRequest("not a Clerk user".into()))?;
    let rows = queries::list_bots_by_submitter(state.db(), user_id).await?;
    Ok(Json(rows.iter().map(bot_to_response).collect()))
}

/// GET /me — return current user info from auth identity.
pub async fn get_me(auth: AuthIdentity) -> AppResult<Json<UserInfoResponse>> {
    match &auth {
        AuthIdentity::BearerToken => Ok(Json(UserInfoResponse {
            user_id: "admin".into(),
            role: "admin".into(),
        })),
        AuthIdentity::ClerkUser { user_id, role } => Ok(Json(UserInfoResponse {
            user_id: user_id.clone(),
            role: role.clone(),
        })),
    }
}
