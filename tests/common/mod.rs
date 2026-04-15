use axum::Router;
use sqlx::SqlitePool;
use bot_council::state::AppState;
use bot_council::config::{Settings, ServerConfig, DatabaseConfig, AuthConfig, HttpClientConfig};

/// Build a test application with an in-memory SQLite database and no auth.
pub async fn test_app() -> (Router, SqlitePool) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let settings = Settings {
        server: ServerConfig { host: "127.0.0.1".into(), port: 0 },
        database: DatabaseConfig { url: "sqlite::memory:".into() },
        auth: AuthConfig { admin_token: "".into() },
        http_client: HttpClientConfig {
            connect_timeout_secs: 5,
            request_timeout_secs: 30,
            max_retries: 0,
            retry_delay_secs: 1,
        },
    };
    let http_client = bot_council::bot_client::build_http_client(&settings.http_client);
    let state = AppState::new(pool.clone(), http_client, settings);
    let app = bot_council::api::router(state);
    (app, pool)
}
