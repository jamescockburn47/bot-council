//! Admin registry endpoints.
//!
//! Admins are stored in the `admins` table in SQLite; membership is checked on
//! every authenticated Clerk request (see `src/api/auth.rs`). Promotion and
//! demotion happen via these endpoints, which are themselves admin-gated.
//!
//! Bootstrap: the first admin is seeded by the operator using the static
//! `admin_token` bearer to POST their own user_id here.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::auth::{RequireAdmin, RequireAuth};
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct AdminEntry {
    pub user_id: String,
    pub granted_at: String,
    pub granted_by: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddAdminRequest {
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct SeenUserEntry {
    pub user_id: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub is_admin: bool,
}

/// GET /admins — list all current admins.
pub async fn list_admins(
    State(state): State<AppState>,
    _admin: RequireAdmin,
) -> AppResult<Json<Vec<AdminEntry>>> {
    let rows = queries::list_admins(state.db()).await?;
    Ok(Json(rows.into_iter().map(|r| AdminEntry {
        user_id: r.user_id,
        granted_at: r.granted_at,
        granted_by: r.granted_by,
    }).collect()))
}

/// POST /admins — promote a user to admin.
///
/// The target user_id must start with `user_` (Clerk format). The caller's
/// own user_id is recorded as `granted_by` for audit.
pub async fn add_admin(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Json(req): Json<AddAdminRequest>,
) -> AppResult<(StatusCode, Json<AdminEntry>)> {
    let target = req.user_id.trim();
    if !target.starts_with("user_") {
        return Err(AppError::BadRequest(
            "user_id must be a Clerk user_id (format user_2...)".into(),
        ));
    }
    let granted_by = admin.0.user_id();
    queries::add_admin(state.db(), target, granted_by).await?;

    // Return the newly-stored row so the UI can render it.
    let rows = queries::list_admins(state.db()).await?;
    let row = rows.into_iter().find(|r| r.user_id == target)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("admin row missing after insert")))?;
    Ok((StatusCode::CREATED, Json(AdminEntry {
        user_id: row.user_id,
        granted_at: row.granted_at,
        granted_by: row.granted_by,
    })))
}

/// DELETE /admins/{user_id} — demote a user.
///
/// An admin cannot demote themselves — prevents accidental lockout. Use a
/// second admin to demote, or the `admin_token` bearer path.
pub async fn remove_admin(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(user_id): Path<String>,
) -> AppResult<StatusCode> {
    if let Some(self_id) = admin.0.user_id() {
        if self_id == user_id {
            return Err(AppError::BadRequest(
                "cannot demote yourself; ask another admin or use the admin_token".into(),
            ));
        }
    }
    let removed = queries::remove_admin(state.db(), &user_id).await?;
    if !removed {
        return Err(AppError::NotFound("admin not found".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// GET /users — list every user that has signed in, newest first, with a flag
/// showing whether each is currently an admin. Used by the admin UI to offer
/// a promote list.
pub async fn list_users(
    State(state): State<AppState>,
    _auth: RequireAuth,
) -> AppResult<Json<Vec<SeenUserEntry>>> {
    // Any signed-in user can see this list; it's just Clerk user_ids with
    // no sensitive content. Admin-only if you want to lock it down later.
    let seen = queries::list_seen_users(state.db()).await?;
    let admins = queries::list_admins(state.db()).await?;
    let admin_set: std::collections::HashSet<&str> =
        admins.iter().map(|r| r.user_id.as_str()).collect();

    Ok(Json(seen.into_iter().map(|r| SeenUserEntry {
        is_admin: admin_set.contains(r.user_id.as_str()),
        user_id: r.user_id,
        first_seen_at: r.first_seen_at,
        last_seen_at: r.last_seen_at,
    }).collect()))
}
