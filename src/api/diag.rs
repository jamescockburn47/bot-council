//! Operator diagnostics (no secrets).

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::api::auth::RequireAdmin;
use crate::state::AppState;

/// Effective model routing for analysis vs final synthesis (admin only).
#[derive(Debug, Serialize)]
pub struct ModelDiagnostics {
    pub analysis_base_url: String,
    pub analysis_model: String,
    pub analysis_connect_timeout_secs: u64,
    pub analysis_request_timeout_secs: u64,
    pub analysis_max_concurrency: usize,
    pub final_synthesis_base_url: String,
    pub final_synthesis_model: String,
    pub final_synthesis_connect_timeout_secs: u64,
    pub final_synthesis_request_timeout_secs: u64,
    pub final_synthesis_warmup_enabled: bool,
    pub final_synthesis_warmup_max_attempts: u32,
    pub final_synthesis_warmup_delay_secs: u64,
    pub legacy_local_synthesis_base_url: String,
    pub legacy_local_synthesis_model: String,
}

/// GET /diag/models — admin-only snapshot of LLM endpoint configuration.
pub async fn get_model_diagnostics(
    State(state): State<AppState>,
    _auth: RequireAdmin,
) -> Json<ModelDiagnostics> {
    let m = &state.settings().models;
    Json(ModelDiagnostics {
        analysis_base_url: m.effective_analysis_base_url().to_string(),
        analysis_model: m.effective_analysis_model().to_string(),
        analysis_connect_timeout_secs: m.analysis_connect_timeout_secs,
        analysis_request_timeout_secs: m.analysis_request_timeout_secs,
        analysis_max_concurrency: m.analysis_max_concurrency,
        final_synthesis_base_url: m.effective_final_synthesis_base_url().to_string(),
        final_synthesis_model: m.effective_final_synthesis_model().to_string(),
        final_synthesis_connect_timeout_secs: m.final_synthesis_connect_timeout_secs,
        final_synthesis_request_timeout_secs: m.final_synthesis_request_timeout_secs,
        final_synthesis_warmup_enabled: m.final_synthesis_warmup_enabled,
        final_synthesis_warmup_max_attempts: m.final_synthesis_warmup_max_attempts,
        final_synthesis_warmup_delay_secs: m.final_synthesis_warmup_delay_secs,
        legacy_local_synthesis_base_url: m.local_synthesis_base_url.clone(),
        legacy_local_synthesis_model: m.local_synthesis_model.clone(),
    })
}
