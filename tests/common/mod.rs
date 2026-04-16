use axum::Router;
use sqlx::SqlitePool;
use bot_council::state::AppState;
use bot_council::config::{Settings, ServerConfig, DatabaseConfig, AuthConfig, HttpClientConfig, ModelsConfig, DebateConfig};

/// Build a test application with an in-memory SQLite database and no auth.
pub async fn test_app() -> (Router, SqlitePool) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let settings = Settings {
        server: ServerConfig { host: "127.0.0.1".into(), port: 0, cors_origins: vec![] },
        database: DatabaseConfig { url: "sqlite::memory:".into() },
        auth: AuthConfig {
            admin_token: "test-admin-token".into(),
            clerk_issuer: "".into(),
            clerk_jwks_url: "".into(),
            admin_user_ids: vec![],
            // 32 bytes = 64 hex chars; deterministic for reproducible tests.
            bot_token_key: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into(),
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
        },
        debate: DebateConfig {
            default_timeout_secs: 30,
            max_retries: 2,
            quorum: 3,
            synthesis_temperature: 0.0,
        },
    };
    let http_client = bot_council::bot_client::build_http_client(&settings.http_client);
    let state = AppState::new(pool.clone(), http_client, settings);
    let app = bot_council::api::router(state);
    (app, pool)
}

use axum::http::HeaderValue;

/// Helper: attach the test admin bearer token to a request builder.
#[allow(dead_code)]
pub fn admin_auth(req: axum::http::request::Builder) -> axum::http::request::Builder {
    req.header("authorization", HeaderValue::from_static("Bearer test-admin-token"))
}
