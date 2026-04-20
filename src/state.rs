use crate::api::events::DebateEvent;
use crate::config::Settings;
use dashmap::DashMap;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Application state shared across all handlers. Cheap to clone (Arc wrapper).
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    db: SqlitePool,
    http_client: reqwest_middleware::ClientWithMiddleware,
    settings: Settings,
    debate_streams: DashMap<String, broadcast::Sender<DebateEvent>>,
    jwks: crate::api::jwks_cache::JwksCache,
    bot_token_key: crate::api::bot_token_crypto::BotTokenKey,
}

impl AppState {
    /// Construct application state from an initialised pool, HTTP client, and settings.
    pub fn new(
        db: SqlitePool,
        http_client: reqwest_middleware::ClientWithMiddleware,
        settings: Settings,
        jwks: crate::api::jwks_cache::JwksCache,
        bot_token_key: crate::api::bot_token_crypto::BotTokenKey,
    ) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                db,
                http_client,
                settings,
                debate_streams: DashMap::new(),
                jwks,
                bot_token_key,
            }),
        }
    }

    /// Return a reference to the SQLite connection pool.
    pub fn db(&self) -> &SqlitePool {
        &self.inner.db
    }

    /// Return a reference to the outbound HTTP client.
    pub fn http_client(&self) -> &reqwest_middleware::ClientWithMiddleware {
        &self.inner.http_client
    }

    /// Return a reference to the loaded settings.
    pub fn settings(&self) -> &Settings {
        &self.inner.settings
    }

    /// Return a reference to the Clerk JWKS cache.
    pub fn jwks(&self) -> &crate::api::jwks_cache::JwksCache {
        &self.inner.jwks
    }

    /// Return a reference to the bot token encryption key.
    pub fn bot_token_key(&self) -> &crate::api::bot_token_crypto::BotTokenKey {
        &self.inner.bot_token_key
    }

    /// Create a broadcast channel for a debate and return the Sender.
    pub fn create_debate_stream(&self, debate_id: &str) -> broadcast::Sender<DebateEvent> {
        let (tx, _rx) = broadcast::channel(64);
        self.inner
            .debate_streams
            .insert(debate_id.to_string(), tx.clone());
        tx
    }

    /// Subscribe to an existing debate stream. Returns None if no active stream.
    pub fn subscribe_debate_stream(
        &self,
        debate_id: &str,
    ) -> Option<broadcast::Receiver<DebateEvent>> {
        self.inner
            .debate_streams
            .get(debate_id)
            .map(|tx| tx.subscribe())
    }

    /// Remove a debate stream from the registry.
    pub fn remove_debate_stream(&self, debate_id: &str) {
        self.inner.debate_streams.remove(debate_id);
    }
}
