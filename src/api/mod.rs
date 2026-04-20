pub mod admins;
pub mod auth;
pub mod bot_token_crypto;
pub mod bots;
pub mod compat;
pub mod jwks_cache;
pub mod debates;
pub mod diag;
pub mod dto;
pub mod events;
pub mod health;
pub mod stream;
pub mod synthesis;
pub mod transcript;

use axum::{Router, http::HeaderValue, routing::{delete, get, patch}};
use tower_http::cors::{Any, CorsLayer};
use axum::http::Method;
use crate::state::AppState;

/// Build the CORS layer from the configured origins.
///
/// If `origins` is empty, returns a permissive layer suitable for development.
/// Otherwise, returns a restrictive layer allowing only the listed origins.
fn cors_layer(origins: &[String]) -> CorsLayer {
    if origins.is_empty() {
        CorsLayer::permissive()
    } else {
        let parsed: Vec<HeaderValue> = origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(parsed)
            .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
            .allow_headers(Any)
    }
}

/// Build the API router with all routes.
pub fn router(state: AppState) -> Router {
    let cors = cors_layer(&state.settings().server.cors_origins);
    Router::new()
        .route("/health", get(health::health))
        .route("/diag/health", get(health::health))
        .route("/diag/models", get(diag::get_model_diagnostics))
        .route("/me", get(bots::get_me))
        .route("/bots/my-submissions", get(bots::my_submissions))
        .route("/bots", get(bots::list_bots).post(bots::create_bot))
        .route("/bots/schema", get(compat::legacy_bot_schema))
        .route("/bots/{id}/history", get(compat::legacy_bot_history))
        .route("/bots/{id}/analytics", get(bots::get_bot_analytics))
        .route("/bots/{id}/approve", patch(bots::approve_bot))
        .route("/bots/{id}/test", patch(bots::test_bot))
        .route("/bots/{id}/reject", patch(bots::reject_bot))
        .route("/bots/{id}/deactivate", patch(bots::deactivate_bot))
        .route("/bots/{id}/reactivate", patch(bots::reactivate_bot))
        .route("/debates", get(debates::list_debates).post(debates::create_debate))
        .route("/debates/{id}", get(debates::get_debate))
        .route("/debates/{id}/transcript", get(transcript::get_transcript))
        .route("/debates/{id}/synthesis", get(synthesis::get_synthesis))
        .route("/debates/{id}/stream", get(stream::stream_debate))
        .route("/admins", get(admins::list_admins).post(admins::add_admin))
        .route("/admins/{user_id}", delete(admins::remove_admin))
        .route("/users", get(admins::list_users))
        .layer(cors)
        .with_state(state)
}
