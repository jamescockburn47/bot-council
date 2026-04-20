pub mod api;
pub mod config;
pub mod db;
pub mod error;
pub mod observability;
pub mod sanitise;
pub mod state;
pub mod types;
pub mod bot_client;
pub mod orchestrator;
pub mod analyser;
pub mod synthesiser;
pub mod scoreboard;

use axum::Router;

/// Build the full application router with state.
pub async fn build_app() -> anyhow::Result<Router> {
    let settings = config::Settings::load()?;
    settings.validate()?;
    let pool = db::init_pool(&settings.database.url).await?;
    let http_client = bot_client::build_http_client(&settings.http_client);

    // Bot token key: parse from config or use zero key when Clerk is disabled.
    // Boot-time validation in Task 13 rejects the zero key when clerk_issuer is set.
    let bot_token_key = api::bot_token_crypto::parse_key_hex(&settings.auth.bot_token_key)
        .unwrap_or_else(|_| api::bot_token_crypto::BotTokenKey::zero());

    // JWKS cache: URL derived from issuer if not explicitly set.
    let jwks_url = if settings.auth.clerk_jwks_url.is_empty() {
        format!("{}/.well-known/jwks.json", settings.auth.clerk_issuer)
    } else {
        settings.auth.clerk_jwks_url.clone()
    };
    let jwks = api::jwks_cache::JwksCache::new(jwks_url);

    if !settings.auth.clerk_issuer.is_empty() {
        let raw_client = reqwest::Client::new();
        if let Err(e) = jwks.refresh(&raw_client).await {
            tracing::warn!(error = %e, "initial JWKS fetch failed; continuing with empty cache");
        }
        api::jwks_cache::spawn_refresh_loop(jwks.clone(), raw_client, 600);
    }

    let state = state::AppState::new(pool, http_client, settings.clone(), jwks, bot_token_key);
    Ok(api::router(state))
}
