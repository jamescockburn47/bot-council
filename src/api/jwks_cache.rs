//! Fetches and caches the Clerk JWKS for JWT signature verification.
//!
//! The key set is hot-swappable via `ArcSwap` so the background refresh task
//! never blocks request handlers. On fetch failure the previous cached set is
//! retained — only a startup failure (empty cache) returns `None`.

use arc_swap::ArcSwap;
use jsonwebtoken::jwk::JwkSet;
use std::sync::Arc;
use std::time::Duration;

/// Cached JWKS keyed against the URL it was fetched from.
#[derive(Debug, Clone)]
pub struct JwksCache {
    inner: Arc<ArcSwap<Option<JwkSet>>>,
    url: String,
}

impl JwksCache {
    /// Create an empty cache bound to the given JWKS URL.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(ArcSwap::from_pointee(None)),
            url: url.into(),
        }
    }

    /// Return the current JWKS, or `None` if never successfully fetched.
    pub fn current(&self) -> Option<Arc<JwkSet>> {
        let guard = self.inner.load();
        guard.as_ref().as_ref().map(|jwks| Arc::new(jwks.clone()))
    }

    /// JWKS URL this cache fetches from.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Fetch and swap in a new JWKS. On failure the existing cache is untouched.
    pub async fn refresh(&self, client: &reqwest::Client) -> anyhow::Result<()> {
        let bytes = client
            .get(&self.url)
            .timeout(Duration::from_secs(10))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let jwks: JwkSet = serde_json::from_slice(&bytes)?;
        self.inner.store(Arc::new(Some(jwks)));
        Ok(())
    }
}

/// Spawn a background task that refreshes the cache every `interval_secs`.
/// Logs errors at WARN but never panics or exits.
pub fn spawn_refresh_loop(cache: JwksCache, client: reqwest::Client, interval_secs: u64) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.tick().await; // skip the immediate first tick — startup already fetched
        loop {
            ticker.tick().await;
            if let Err(e) = cache.refresh(&client).await {
                tracing::warn!(error = %e, url = %cache.url(), "JWKS refresh failed");
            } else {
                tracing::debug!(url = %cache.url(), "JWKS refreshed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cache_returns_none() {
        let cache = JwksCache::new("https://example.invalid/.well-known/jwks.json");
        assert!(cache.current().is_none());
    }
}
