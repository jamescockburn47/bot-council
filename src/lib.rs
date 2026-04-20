pub mod analyser;
pub mod api;
pub mod bot_client;
pub mod config;
pub mod db;
pub mod error;
pub mod observability;
pub mod orchestrator;
pub mod sanitise;
pub mod scoreboard;
pub mod state;
pub mod synthesiser;
pub mod types;

use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

/// Build the full application router with state.
///
/// The API routes are mounted under `/api/*`; everything else falls through
/// to the static SvelteKit build (served from `FRONTEND_DIST_DIR`, default
/// `./frontend/build`) with `index.html` as the SPA fallback.
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
        // Hard-fail if JWKS can't be loaded at boot. Previously logged a warn
        // and continued with an empty cache, which meant every authenticated
        // request returned 500 "JWKS not yet loaded" for up to the refresh
        // interval (600s) whenever Clerk DNS was briefly flaky at startup.
        // Systemd's StartLimitBurst=5 / StartLimitIntervalSec=300 (see
        // deploy/bot-council.service) bounds retries so a sustained Clerk
        // outage surfaces to monitoring instead of silently degrading.
        retry_initial_jwks(&jwks, &raw_client).await?;
        api::jwks_cache::spawn_refresh_loop(jwks.clone(), raw_client, 600);
    }

    let state = state::AppState::new(pool, http_client, settings.clone(), jwks, bot_token_key);

    // Two instances of the same router so we can mount routes at both `/*` and
    // `/api/*` without Router internal-state sharing complaints. The root mount
    // is a TRANSITIONAL backward-compat path for the currently-deployed Vercel
    // proxy (which rewrites `api.lqcouncil.com/*` to EVO `/*`). Once the
    // Vercel proxy is retired in Phase F, remove `.merge(api_root)` so the
    // backend exposes `/api/*` only.
    let api_nested = api::router(state.clone());
    let api_root = api::router(state);

    // Static frontend: SvelteKit adapter-static output. Falls back to
    // `index.html` for any path that isn't a real file (SPA client-side routing).
    let static_dir =
        std::env::var("FRONTEND_DIST_DIR").unwrap_or_else(|_| "./frontend/build".to_string());
    let index_path = format!("{static_dir}/index.html");
    let static_service = ServeDir::new(&static_dir).not_found_service(ServeFile::new(&index_path));

    Ok(Router::new()
        .nest("/api", api_nested)
        .merge(api_root)
        .fallback_service(static_service))
}

/// Retry the initial JWKS fetch with exponential backoff.
///
/// Attempts: 0s, +1s, +2s, +4s, +8s, +16s — roughly 31s of total wait
/// before we give up and `anyhow::bail!`. Each attempt has the 10s timeout
/// that [`api::jwks_cache::JwksCache::refresh`] applies internally, so the
/// worst-case wall time is ~91s (6 × 10s request timeouts + the backoff
/// waits). Systemd's `TimeoutStartSec=60` means a truly dead Clerk will
/// be flagged as a startup failure rather than a hung service.
async fn retry_initial_jwks(
    jwks: &api::jwks_cache::JwksCache,
    client: &reqwest::Client,
) -> anyhow::Result<()> {
    let mut delay = std::time::Duration::from_secs(1);
    let max_delay = std::time::Duration::from_secs(16);
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=6 {
        match jwks.refresh(client).await {
            Ok(()) => {
                if attempt > 1 {
                    tracing::info!(attempt, "JWKS loaded after backoff");
                }
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(attempt, error = %e, "initial JWKS fetch failed, retrying");
                last_err = Some(e);
                if attempt < 6 {
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay * 2, max_delay);
                }
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| anyhow::anyhow!("JWKS never loaded"))
        .context("gave up loading JWKS after 6 attempts"))
}
