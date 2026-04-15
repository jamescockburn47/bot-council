pub mod auth;
pub mod bots;
pub mod debates;
pub mod dto;
pub mod health;

use axum::{Router, routing::get};
use crate::state::AppState;

/// Build the API router with all routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/bots", get(bots::list_bots).post(bots::create_bot))
        .route("/debates", get(debates::list_debates).post(debates::create_debate))
        .route("/debates/{id}", get(debates::get_debate))
        .with_state(state)
}
