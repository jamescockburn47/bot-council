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
/// Supports bearer token (bots/admin) and Clerk JWT (frontend users).
/// Dev mode: if both `admin_token` and `clerk_issuer` are empty, auth is disabled.
#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub admin_token: String,
    pub clerk_jwks_url: String,
    pub clerk_issuer: String,
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
}
