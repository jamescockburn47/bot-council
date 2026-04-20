//! `GET /api/config.json` — runtime config surfaced to the frontend.
//!
//! Replaces the old Vercel build-time env vars (`PUBLIC_API_URL`,
//! `PUBLIC_CLERK_PUBLISHABLE_KEY`, `PUBLIC_SENTRY_ENVIRONMENT`). Public by
//! design — everything returned is safe to ship to an unauthenticated browser.

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::state::AppState;

/// Runtime config payload for the static frontend.
#[derive(Debug, Serialize)]
pub struct PublicConfigResponse {
    /// Clerk publishable key (pk_live_* or pk_test_*). Safe to ship publicly.
    pub publishable_key: String,
    /// Base URL for API calls. Frontend uses relative `/api`.
    pub api_base: &'static str,
    /// Sentry environment tag ("prod", "staging", "dev", ...).
    pub sentry_environment: String,
    /// Release tag — git SHA baked into the backend via `SENTRY_RELEASE`,
    /// or the package version for local builds. Used for cache busting
    /// and Sentry release correlation.
    pub release: String,
}

/// Handler for `GET /config.json` (mounted under `/api` in production).
pub async fn get_config_json(State(state): State<AppState>) -> Json<PublicConfigResponse> {
    let s = state.settings();
    let release = std::env::var("SENTRY_RELEASE")
        .ok()
        .filter(|r| !r.trim().is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").into());
    Json(PublicConfigResponse {
        publishable_key: s.auth.clerk_publishable_key.clone(),
        api_base: "/api",
        sentry_environment: s.sentry.environment.clone(),
        release,
    })
}
