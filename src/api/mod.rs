pub mod auth;
pub mod bots;
pub mod debates;
pub mod dto;
pub mod health;
pub mod synthesis;
pub mod transcript;

use axum::{Router, routing::get};
use crate::state::AppState;

/// Build the API router with all routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/bots", get(bots::list_bots).post(bots::create_bot))
        .route("/debates", get(debates::list_debates).post(debates::create_debate))
        .route("/debates/{id}", get(debates::get_debate))
        .route("/debates/{id}/transcript", get(transcript::get_transcript))
        .route("/debates/{id}/synthesis", get(synthesis::get_synthesis))
        .with_state(state)
}
