use serde::Deserialize;

/// Top-level application settings. Loaded from config/default.toml + env vars.
#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub http_client: HttpClientConfig,
    pub models: ModelsConfig,
    pub debate: DebateConfig,
}

/// HTTP server bind configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Allowed CORS origins. Empty list enables permissive mode (dev).
    pub cors_origins: Vec<String>,
}

/// Database connection configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

/// Authentication configuration.
/// Supports bearer token (admin CLI/bots) and Clerk JWT (frontend users).
#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    /// Static bearer token granting admin. Empty string disables this path.
    pub admin_token: String,
    /// Base URL of the Clerk issuer, e.g. `https://<app>.clerk.accounts.dev`.
    pub clerk_issuer: String,
    /// Clerk JWKS URL. If empty, derived from `clerk_issuer` as
    /// `{issuer}/.well-known/jwks.json`.
    pub clerk_jwks_url: String,
    /// Clerk user_ids (format `user_2...`) granted admin role.
    #[serde(default)]
    pub admin_user_ids: Vec<String>,
    /// 64-character hex string (32 bytes) — AES-256 key for bot token
    /// encryption. Required when Clerk is configured.
    pub bot_token_key: String,
}

/// Outbound HTTP client tuning.
#[derive(Debug, Deserialize, Clone)]
pub struct HttpClientConfig {
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub max_retries: u32,
    pub retry_delay_secs: u64,
}

/// LLM model configuration for MiniMax (analysis) and Opus (synthesis).
#[derive(Debug, Deserialize, Clone)]
pub struct ModelsConfig {
    pub minimax_api_key: String,
    pub minimax_model: String,
    pub minimax_base_url: String,
    pub opus_api_key: String,
    pub opus_model: String,
}

/// Debate protocol tuning.
#[derive(Debug, Deserialize, Clone)]
pub struct DebateConfig {
    pub default_timeout_secs: u64,
    pub max_retries: u32,
    pub quorum: usize,
    pub synthesis_temperature: f64,
}

impl Settings {
    /// Load settings from config/default.toml, overridden by APP__* env vars.
    pub fn load() -> anyhow::Result<Self> {
        let config = config::Config::builder()
            .add_source(config::File::with_name("config/default"))
            .add_source(
                config::Environment::with_prefix("APP")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;
        Ok(config.try_deserialize()?)
    }

    /// Fail-fast validation of boot-time configuration invariants.
    /// Returns the first error found; the caller should refuse to start.
    pub fn validate(&self) -> anyhow::Result<()> {
        let a = &self.auth;

        // 1. At least one auth path must be configured (tests skip this).
        if a.admin_token.is_empty() && a.clerk_issuer.is_empty() && !cfg!(test) {
            anyhow::bail!(
                "auth.admin_token OR auth.clerk_issuer must be set. \
                 Dev-mode auto-admin has been removed."
            );
        }

        // 2. Clerk path requires admin_user_ids and bot_token_key.
        if !a.clerk_issuer.is_empty() {
            if a.admin_user_ids.is_empty() {
                anyhow::bail!(
                    "auth.clerk_issuer is set but auth.admin_user_ids is empty; \
                     no one would have admin privileges"
                );
            }
            for id in &a.admin_user_ids {
                if !id.starts_with("user_") {
                    anyhow::bail!(
                        "auth.admin_user_ids contains '{id}', which does not look \
                         like a Clerk user_id (expected format: user_2...)"
                    );
                }
            }
            if a.bot_token_key.is_empty() {
                anyhow::bail!(
                    "auth.clerk_issuer is set but auth.bot_token_key is not; \
                     bot tokens cannot be encrypted"
                );
            }
            crate::api::bot_token_crypto::parse_key_hex(&a.bot_token_key).map_err(|_| {
                anyhow::anyhow!(
                    "auth.bot_token_key must be exactly 64 hex characters (32 bytes)"
                )
            })?;
        }

        Ok(())
    }
}
