use bot_council::config::{
    AuthConfig, DatabaseConfig, DebateConfig, HttpClientConfig, ModelsConfig, SentryConfig,
    ServerConfig, Settings,
};

fn base() -> Settings {
    Settings {
        server: ServerConfig { host: "".into(), port: 0, cors_origins: vec![] },
        database: DatabaseConfig { url: "".into() },
        auth: AuthConfig {
            admin_token: "".into(),
            clerk_issuer: "".into(),
            clerk_jwks_url: "".into(),
            bot_token_key: "".into(),
            test_mode: false,
        },
        http_client: HttpClientConfig {
            connect_timeout_secs: 1,
            request_timeout_secs: 1,
            max_retries: 0,
            retry_delay_secs: 1,
        },
        models: ModelsConfig {
            minimax_api_key: "".into(),
            minimax_model: "".into(),
            minimax_base_url: "".into(),
            opus_api_key: "".into(),
            opus_model: "".into(),
        },
        debate: DebateConfig {
            default_timeout_secs: 1,
            max_retries: 0,
            quorum: 3,
            synthesis_temperature: 0.0,
        },
        sentry: SentryConfig {
            dsn: "".into(),
            environment: "test".into(),
            traces_sample_rate: 0.0,
        },
    }
}

#[test]
fn rejects_missing_bot_token_key_when_clerk_set() {
    let mut s = base();
    s.auth.clerk_issuer = "https://example.clerk.accounts.dev".into();
    let err = s.validate().unwrap_err().to_string();
    assert!(err.contains("bot_token_key"), "error was: {err}");
}

#[test]
fn rejects_bot_token_key_wrong_length() {
    let mut s = base();
    s.auth.clerk_issuer = "https://example.clerk.accounts.dev".into();
    s.auth.bot_token_key = "abcd".into();
    let err = s.validate().unwrap_err().to_string();
    assert!(err.contains("64 hex"), "error was: {err}");
}

#[test]
fn accepts_bearer_only_config() {
    let mut s = base();
    s.auth.admin_token = "some-secret".into();
    assert!(s.validate().is_ok());
}

#[test]
fn accepts_valid_clerk_config_without_preset_admins() {
    let mut s = base();
    s.auth.clerk_issuer = "https://example.clerk.accounts.dev".into();
    s.auth.bot_token_key = "0".repeat(64);
    // No admin_user_ids required any more — bootstrap happens in-app.
    assert!(s.validate().is_ok());
}

#[test]
fn rejects_test_mode_with_clerk() {
    let mut s = base();
    s.auth.clerk_issuer = "https://example.clerk.accounts.dev".into();
    s.auth.bot_token_key = "0".repeat(64);
    s.auth.test_mode = true;
    let err = s.validate().unwrap_err().to_string();
    assert!(err.contains("test_mode"), "error was: {err}");
}
