use axum::Router;
use bot_council::config::{
    AuthConfig, DatabaseConfig, DebateConfig, HttpClientConfig, ModelsConfig, SentryConfig,
    ServerConfig, Settings,
};
use bot_council::state::AppState;
use sqlx::SqlitePool;

/// Build a test application with an in-memory SQLite database and no auth.
pub async fn test_app() -> (Router, SqlitePool) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let settings = Settings {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
            cors_origins: vec![],
        },
        database: DatabaseConfig {
            url: "sqlite::memory:".into(),
        },
        auth: AuthConfig {
            admin_token: "test-admin-token".into(),
            clerk_issuer: "".into(),
            clerk_jwks_url: "".into(),
            // 32 bytes = 64 hex chars; deterministic for reproducible tests.
            bot_token_key: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
                .into(),
            test_mode: true,
            clerk_publishable_key: "pk_test_Y29uZmlnLmpzb24tdGVzdA".into(),
        },
        http_client: HttpClientConfig {
            connect_timeout_secs: 5,
            request_timeout_secs: 30,
            max_retries: 0,
            retry_delay_secs: 1,
        },
        models: ModelsConfig {
            minimax_api_key: "test-minimax-key".into(),
            minimax_model: "M2.7".into(),
            minimax_base_url: "http://localhost:9999".into(),
            opus_api_key: "test-opus-key".into(),
            opus_model: "claude-opus-4-6".into(),
            analysis_base_url: "http://localhost:8086".into(),
            analysis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
            analysis_connect_timeout_secs: 5,
            analysis_request_timeout_secs: 60,
            analysis_max_concurrency: 2,
            final_synthesis_base_url: "http://localhost:8087".into(),
            final_synthesis_model: "Qwen3.5-122B-A10B-UD-Q5_K_XL".into(),
            final_synthesis_connect_timeout_secs: 10,
            final_synthesis_request_timeout_secs: 300,
            final_synthesis_warmup_enabled: false,
            final_synthesis_warmup_max_attempts: 0,
            final_synthesis_warmup_delay_secs: 1,
            local_synthesis_base_url: "http://localhost:8086".into(),
            local_synthesis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
        },
        debate: DebateConfig {
            default_timeout_secs: 30,
            max_retries: 2,
            quorum: 3,
            synthesis_temperature: 0.0,
            test_mode_simple: false,
        },
        sentry: SentryConfig {
            dsn: "".into(),
            environment: "test".into(),
            traces_sample_rate: 0.0,
        },
    };
    let http_client = bot_council::bot_client::build_http_client(&settings.http_client);
    let jwks = bot_council::api::jwks_cache::JwksCache::new("http://localhost/unused");
    let bot_token_key = bot_council::api::bot_token_crypto::BotTokenKey::zero();
    let state = AppState::new(pool.clone(), http_client, settings, jwks, bot_token_key);
    let app = bot_council::api::router(state);
    (app, pool)
}

/// Build a test application with simple test-mode debate behavior enabled.
pub async fn test_app_simple_mode() -> (Router, SqlitePool) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let settings = Settings {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
            cors_origins: vec![],
        },
        database: DatabaseConfig {
            url: "sqlite::memory:".into(),
        },
        auth: AuthConfig {
            admin_token: "test-admin-token".into(),
            clerk_issuer: "".into(),
            clerk_jwks_url: "".into(),
            bot_token_key: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
                .into(),
            test_mode: true,
            clerk_publishable_key: "pk_test_Y29uZmlnLmpzb24tdGVzdA".into(),
        },
        http_client: HttpClientConfig {
            connect_timeout_secs: 5,
            request_timeout_secs: 30,
            max_retries: 0,
            retry_delay_secs: 1,
        },
        models: ModelsConfig {
            minimax_api_key: "test-minimax-key".into(),
            minimax_model: "M2.7".into(),
            minimax_base_url: "http://localhost:9999".into(),
            opus_api_key: "test-opus-key".into(),
            opus_model: "claude-opus-4-6".into(),
            analysis_base_url: "http://localhost:8086".into(),
            analysis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
            analysis_connect_timeout_secs: 5,
            analysis_request_timeout_secs: 60,
            analysis_max_concurrency: 2,
            final_synthesis_base_url: "http://localhost:8087".into(),
            final_synthesis_model: "Qwen3.5-122B-A10B-UD-Q5_K_XL".into(),
            final_synthesis_connect_timeout_secs: 10,
            final_synthesis_request_timeout_secs: 300,
            final_synthesis_warmup_enabled: false,
            final_synthesis_warmup_max_attempts: 0,
            final_synthesis_warmup_delay_secs: 1,
            local_synthesis_base_url: "http://localhost:8086".into(),
            local_synthesis_model: "gemma-4-31B-it-Q4_K_M.gguf".into(),
        },
        debate: DebateConfig {
            default_timeout_secs: 30,
            max_retries: 2,
            quorum: 3,
            synthesis_temperature: 0.0,
            test_mode_simple: true,
        },
        sentry: SentryConfig {
            dsn: "".into(),
            environment: "test".into(),
            traces_sample_rate: 0.0,
        },
    };
    let http_client = bot_council::bot_client::build_http_client(&settings.http_client);
    let jwks = bot_council::api::jwks_cache::JwksCache::new("http://localhost/unused");
    let bot_token_key = bot_council::api::bot_token_crypto::BotTokenKey::zero();
    let state = AppState::new(pool.clone(), http_client, settings, jwks, bot_token_key);
    let app = bot_council::api::router(state);
    (app, pool)
}

use axum::http::HeaderValue;

/// Helper: attach the test admin bearer token to a request builder.
#[allow(dead_code)]
pub fn admin_auth(req: axum::http::request::Builder) -> axum::http::request::Builder {
    req.header(
        "authorization",
        HeaderValue::from_static("Bearer test-admin-token"),
    )
}

/// Build a test application with every MiniMax-backed model endpoint pointed
/// at the supplied mock server URL. Used by end-to-end tests that need to
/// observe or inject extraction / analysis / synthesis model responses.
#[allow(dead_code)]
pub async fn test_app_with_minimax_url(minimax_url: &str) -> (Router, SqlitePool) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let settings = Settings {
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
            cors_origins: vec![],
        },
        database: DatabaseConfig {
            url: "sqlite::memory:".into(),
        },
        auth: AuthConfig {
            admin_token: "test-admin-token".into(),
            clerk_issuer: "".into(),
            clerk_jwks_url: "".into(),
            bot_token_key: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"
                .into(),
            test_mode: true,
            clerk_publishable_key: "pk_test_Y29uZmlnLmpzb24tdGVzdA".into(),
        },
        http_client: HttpClientConfig {
            connect_timeout_secs: 5,
            request_timeout_secs: 30,
            max_retries: 0,
            retry_delay_secs: 1,
        },
        models: ModelsConfig {
            minimax_api_key: "test-minimax-key".into(),
            minimax_model: "M2.7".into(),
            minimax_base_url: minimax_url.to_string(),
            opus_api_key: "test-opus-key".into(),
            opus_model: "claude-opus-4-6".into(),
            analysis_base_url: minimax_url.to_string(),
            analysis_model: "test-analysis-model".into(),
            analysis_connect_timeout_secs: 5,
            analysis_request_timeout_secs: 60,
            analysis_max_concurrency: 2,
            final_synthesis_base_url: minimax_url.to_string(),
            final_synthesis_model: "test-synthesis-model".into(),
            final_synthesis_connect_timeout_secs: 10,
            final_synthesis_request_timeout_secs: 300,
            final_synthesis_warmup_enabled: false,
            final_synthesis_warmup_max_attempts: 0,
            final_synthesis_warmup_delay_secs: 1,
            local_synthesis_base_url: minimax_url.to_string(),
            local_synthesis_model: "test-local-synthesis-model".into(),
        },
        debate: DebateConfig {
            default_timeout_secs: 30,
            max_retries: 2,
            quorum: 3,
            synthesis_temperature: 0.0,
            test_mode_simple: false,
        },
        sentry: SentryConfig {
            dsn: "".into(),
            environment: "test".into(),
            traces_sample_rate: 0.0,
        },
    };
    let http_client = bot_council::bot_client::build_http_client(&settings.http_client);
    let jwks = bot_council::api::jwks_cache::JwksCache::new("http://localhost/unused");
    let bot_token_key = bot_council::api::bot_token_crypto::BotTokenKey::zero();
    let state = AppState::new(pool.clone(), http_client, settings, jwks, bot_token_key);
    let app = bot_council::api::router(state);
    (app, pool)
}
