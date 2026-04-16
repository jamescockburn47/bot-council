use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use sha2::{Sha256, Digest};
use crate::api::auth::{AuthIdentity, RequireAdmin};
use crate::api::dto::{CreateBotRequest, BotResponse, UserInfoResponse, RejectBotRequest};
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
        status: row.status.clone(),
        description: row.description.clone(),
        submitted_by: row.submitted_by.clone(),
        rejection_reason: row.rejection_reason.clone(),
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

/// Send a smoke-test request to a bot's endpoint before approval.
///
/// Sends a minimal POST with a dummy session, checks that the response is
/// valid JSON containing a string `response` field. Uses a 30-second timeout.
/// The harness does NOT send a bearer token — only the hash is stored, not the
/// raw token, and this call is intentionally unauthenticated.
async fn smoke_test_bot(
    client: &reqwest_middleware::ClientWithMiddleware,
    bot: &BotRow,
) -> Result<(), String> {
    let body = serde_json::json!({
        "session_id": "smoke-test", "round": 0, "role": "proponent",
        "context": [],
        "prompt": "Smoke test: respond with any valid JSON containing a 'response' field."
    });
    let response = client
        .post(&bot.endpoint_url)
        .timeout(std::time::Duration::from_secs(30))
        .json(&body)
        .send().await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = response.status();
    if !status.is_success() { return Err(format!("bot returned HTTP {status}")); }
    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("response is not valid JSON: {e}"))?;
    match json.get("response") {
        Some(serde_json::Value::String(_)) => Ok(()),
        Some(other) => Err(format!("'response' field has wrong type: expected string, got {other}")),
        None => Err("response JSON missing 'response' field".into()),
    }
}

/// Shared transition helper: maps `transition_bot_status` results to either the
/// updated row, a 404 (bot missing), or a 409 (current state not in expected_from).
async fn do_transition(
    state: &AppState,
    admin: &RequireAdmin,
    id: &str,
    expected_from: &[&str],
    new_status: &str,
    rejection_reason: Option<&str>,
) -> AppResult<BotRow> {
    let reviewer = admin.0.user_id();
    let updated = queries::transition_bot_status(
        state.db(), id, expected_from, new_status, reviewer, rejection_reason,
    ).await?;
    match updated {
        Some(row) => Ok(row),
        None => match queries::get_bot(state.db(), id).await? {
            None => Err(AppError::NotFound("bot not found".into())),
            Some(row) => Err(AppError::Conflict(format!(
                "bot is in state '{}', expected one of {:?}",
                row.status, expected_from
            ))),
        },
    }
}

/// PATCH /bots/{id}/approve — admin runs the smoke test, then transitions to
/// `active` on success or `smoke_test_failed` on failure (storing the reason).
pub async fn approve_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let bot = queries::get_bot(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    if !matches!(bot.status.as_str(), "pending" | "smoke_test_failed") {
        return Err(AppError::Conflict(format!(
            "bot is in state '{}', expected 'pending' or 'smoke_test_failed'",
            bot.status
        )));
    }
    match smoke_test_bot(state.http_client(), &bot).await {
        Ok(()) => {
            let row = do_transition(
                &state, &admin, &id,
                &["pending", "smoke_test_failed"], "active", None,
            ).await?;
            Ok(Json(bot_to_response(&row)))
        }
        Err(reason) => {
            let row = do_transition(
                &state, &admin, &id,
                &["pending", "smoke_test_failed"], "smoke_test_failed",
                Some(&reason),
            ).await?;
            Ok(Json(bot_to_response(&row)))
        }
    }
}

/// PATCH /bots/{id}/reject — admin rejects a pending or smoke-test-failed bot
/// with a human-readable reason (10–500 chars).
pub async fn reject_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
    Json(req): Json<RejectBotRequest>,
) -> AppResult<Json<BotResponse>> {
    let reason = req.reason.trim();
    if reason.len() < 10 {
        return Err(AppError::BadRequest("reason must be at least 10 characters".into()));
    }
    if reason.len() > 500 {
        return Err(AppError::BadRequest("reason must be at most 500 characters".into()));
    }
    let row = do_transition(
        &state, &admin, &id,
        &["pending", "smoke_test_failed"], "rejected", Some(reason),
    ).await?;
    Ok(Json(bot_to_response(&row)))
}

/// PATCH /bots/{id}/deactivate — deactivate an active bot (admin only).
pub async fn deactivate_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let row = do_transition(&state, &admin, &id, &["active"], "inactive", None).await?;
    Ok(Json(bot_to_response(&row)))
}

/// PATCH /bots/{id}/reactivate — reactivate an inactive bot (admin only).
pub async fn reactivate_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let row = do_transition(&state, &admin, &id, &["inactive"], "active", None).await?;
    Ok(Json(bot_to_response(&row)))
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
        AuthIdentity::Admin { user_id, .. } => Ok(Json(UserInfoResponse {
            user_id: user_id.clone().unwrap_or_else(|| "admin".into()),
            role: "admin".into(),
        })),
        AuthIdentity::Participant { user_id } => Ok(Json(UserInfoResponse {
            user_id: user_id.clone(),
            role: "member".into(),
        })),
    }
}
