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
    pub sentry: SentryConfig,
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
///
/// Admin identities are stored in the `admins` table and managed at runtime
/// via POST/DELETE /admins — no config-driven allowlist.
#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    /// Static bearer token granting admin. Empty string disables this path.
    /// Also used to bootstrap the first in-app admin via POST /admins.
    pub admin_token: String,
    /// Base URL of the Clerk issuer, e.g. `https://<app>.clerk.accounts.dev`.
    pub clerk_issuer: String,
    /// Clerk JWKS URL. If empty, derived from `clerk_issuer` as
    /// `{issuer}/.well-known/jwks.json`.
    pub clerk_jwks_url: String,
    /// 64-character hex string (32 bytes) — AES-256 key for bot token
    /// encryption. Required when Clerk is configured.
    pub bot_token_key: String,
    /// Test-only auth backdoor. When true, `Authorization: Bearer admin:<uid>`
    /// is accepted as Admin with that user_id, and any other bearer value is
    /// accepted as a Participant with `user_id = <token>`. Defaults to false.
    /// `validate()` refuses to boot when both `test_mode` and `clerk_issuer`
    /// are set, making it impossible to enable in any real Clerk deployment.
    #[serde(default)]
    pub test_mode: bool,
    /// Clerk publishable key surfaced to the frontend at runtime via
    /// `GET /api/config.json`. Public by design (pk_live_* / pk_test_*).
    /// Replaces the old Vercel build-time PUBLIC_CLERK_PUBLISHABLE_KEY env var.
    #[serde(default)]
    pub clerk_publishable_key: String,
}

/// Outbound HTTP client tuning.
#[derive(Debug, Deserialize, Clone)]
pub struct HttpClientConfig {
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub max_retries: u32,
    pub retry_delay_secs: u64,
}

/// LLM model configuration for analysis and synthesis calls.
#[derive(Debug, Deserialize, Clone)]
pub struct ModelsConfig {
    pub minimax_api_key: String,
    pub minimax_model: String,
    pub minimax_base_url: String,
    pub opus_api_key: String,
    pub opus_model: String,
    #[serde(default = "default_analysis_base_url")]
    pub analysis_base_url: String,
    #[serde(default = "default_analysis_model")]
    pub analysis_model: String,
    #[serde(default = "default_analysis_connect_timeout_secs")]
    pub analysis_connect_timeout_secs: u64,
    #[serde(default = "default_analysis_request_timeout_secs")]
    pub analysis_request_timeout_secs: u64,
    #[serde(default = "default_analysis_max_concurrency")]
    pub analysis_max_concurrency: usize,
    #[serde(default = "default_final_synthesis_base_url")]
    pub final_synthesis_base_url: String,
    #[serde(default = "default_final_synthesis_model")]
    pub final_synthesis_model: String,
    #[serde(default = "default_final_synthesis_connect_timeout_secs")]
    pub final_synthesis_connect_timeout_secs: u64,
    #[serde(default = "default_final_synthesis_request_timeout_secs")]
    pub final_synthesis_request_timeout_secs: u64,
    #[serde(default = "default_final_synthesis_warmup_enabled")]
    pub final_synthesis_warmup_enabled: bool,
    #[serde(default = "default_final_synthesis_warmup_max_attempts")]
    pub final_synthesis_warmup_max_attempts: u32,
    #[serde(default = "default_final_synthesis_warmup_delay_secs")]
    pub final_synthesis_warmup_delay_secs: u64,
    #[serde(default = "default_local_synthesis_base_url")]
    pub local_synthesis_base_url: String,
    #[serde(default = "default_local_synthesis_model")]
    pub local_synthesis_model: String,
}

fn default_analysis_base_url() -> String {
    default_local_synthesis_base_url()
}

fn default_analysis_model() -> String {
    default_local_synthesis_model()
}

fn default_analysis_connect_timeout_secs() -> u64 {
    5
}

fn default_analysis_request_timeout_secs() -> u64 {
    120
}

fn default_analysis_max_concurrency() -> usize {
    2
}

fn default_final_synthesis_base_url() -> String {
    default_local_synthesis_base_url()
}

fn default_final_synthesis_model() -> String {
    default_local_synthesis_model()
}

fn default_final_synthesis_connect_timeout_secs() -> u64 {
    10
}

fn default_final_synthesis_request_timeout_secs() -> u64 {
    900
}

fn default_final_synthesis_warmup_enabled() -> bool {
    true
}

fn default_final_synthesis_warmup_max_attempts() -> u32 {
    // Bounded by default so a dead final-synthesis port cannot wedge every
    // debate in an infinite warmup loop (proxy 502 / "API unstable").
    // Set to 0 for infinite retries (block-until-ready) when the 122B server
    // is guaranteed to come up.
    24
}

fn default_final_synthesis_warmup_delay_secs() -> u64 {
    5
}

fn default_local_synthesis_base_url() -> String {
    "http://127.0.0.1:8086".into()
}

fn default_local_synthesis_model() -> String {
    "gemma-4-31B-it-Q4_K_M.gguf".into()
}

impl ModelsConfig {
    pub fn effective_analysis_base_url(&self) -> &str {
        if self.analysis_base_url.trim().is_empty() {
            &self.local_synthesis_base_url
        } else {
            &self.analysis_base_url
        }
    }

    pub fn effective_analysis_model(&self) -> &str {
        if self.analysis_model.trim().is_empty() {
            &self.local_synthesis_model
        } else {
            &self.analysis_model
        }
    }

    pub fn effective_final_synthesis_base_url(&self) -> &str {
        if self.final_synthesis_base_url.trim().is_empty() {
            &self.local_synthesis_base_url
        } else {
            &self.final_synthesis_base_url
        }
    }

    pub fn effective_final_synthesis_model(&self) -> &str {
        if self.final_synthesis_model.trim().is_empty() {
            &self.local_synthesis_model
        } else {
            &self.final_synthesis_model
        }
    }
}

/// Debate protocol tuning.
#[derive(Debug, Deserialize, Clone)]
pub struct DebateConfig {
    pub default_timeout_secs: u64,
    pub max_retries: u32,
    pub quorum: usize,
    pub synthesis_temperature: f64,
    #[serde(default)]
    pub test_mode_simple: bool,
}

/// Sentry error-tracking configuration. Empty DSN disables Sentry entirely.
#[derive(Debug, Deserialize, Clone)]
pub struct SentryConfig {
    /// Sentry DSN. Empty string disables Sentry init (safe no-op).
    pub dsn: String,
    /// Environment tag attached to all events (e.g. "prod", "staging").
    pub environment: String,
    /// Performance tracing sample rate. 0.0 disables traces. Error events
    /// are always captured at 100% when the DSN is set.
    pub traces_sample_rate: f32,
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
        let auth = &self.auth;

        // 1. At least one auth path must be configured (tests skip this).
        if auth.admin_token.is_empty() && auth.clerk_issuer.is_empty() && !cfg!(test) {
            anyhow::bail!(
                "auth.admin_token OR auth.clerk_issuer must be set. \
                 Dev-mode auto-admin has been removed."
            );
        }

        // 2. Clerk path requires a bot_token_key. Admin membership is managed
        //    at runtime via the `admins` table, so no allowlist check here.
        //    First admin is bootstrapped via the admin_token bearer POSTing to
        //    /admins after sign-in — see docs/deploy-clerk-auth-rollout.md.
        if !auth.clerk_issuer.is_empty() {
            if auth.bot_token_key.is_empty() {
                anyhow::bail!(
                    "auth.clerk_issuer is set but auth.bot_token_key is not; \
                     bot tokens cannot be encrypted"
                );
            }
            crate::api::bot_token_crypto::parse_key_hex(&auth.bot_token_key).map_err(|_| {
                anyhow::anyhow!(
                    "auth.bot_token_key must be exactly 64 hex characters (32 bytes)"
                )
            })?;
        }

        // 3. test_mode is mutually exclusive with a real Clerk deployment.
        //    Refusing to boot makes it impossible to accidentally expose the
        //    backdoor in production.
        if auth.test_mode && !auth.clerk_issuer.is_empty() {
            anyhow::bail!(
                "auth.test_mode must not be enabled when auth.clerk_issuer is set"
            );
        }

        Ok(())
    }
}
