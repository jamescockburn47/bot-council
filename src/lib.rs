pub mod api;
pub mod config;
pub mod db;
pub mod error;
pub mod state;
pub mod types;
pub mod bot_client;
pub mod orchestrator;

use axum::Router;

/// Build the full application router with state.
pub async fn build_app() -> anyhow::Result<Router> {
    let settings = config::Settings::load()?;
    let pool = db::init_pool(&settings.database.url).await?;
    let http_client = bot_client::build_http_client(&settings.http_client);
    let state = state::AppState::new(pool, http_client, settings.clone());
    Ok(api::router(state))
}
