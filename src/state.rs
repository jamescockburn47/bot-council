use std::sync::Arc;
use sqlx::SqlitePool;
use crate::config::Settings;

/// Application state shared across all handlers. Cheap to clone (Arc wrapper).
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    pub db: SqlitePool,
    pub http_client: reqwest_middleware::ClientWithMiddleware,
    pub settings: Settings,
}

impl AppState {
    /// Construct application state from an initialised pool, HTTP client, and settings.
    pub fn new(
        db: SqlitePool,
        http_client: reqwest_middleware::ClientWithMiddleware,
        settings: Settings,
    ) -> Self {
        Self {
            inner: Arc::new(AppStateInner { db, http_client, settings }),
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
}
