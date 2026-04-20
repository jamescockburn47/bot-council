pub mod models;
pub mod queries;
pub mod queries_phase1;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

/// Initialise the SQLite connection pool and run migrations.
pub async fn init_pool(url: &str) -> anyhow::Result<SqlitePool> {
    if let Some(path) = url.strip_prefix("sqlite:") {
        let path = path.split('?').next().unwrap_or(path);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await?;

    sqlx::query("PRAGMA journal_mode=WAL")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA synchronous=NORMAL")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA busy_timeout=5000")
        .execute(&pool)
        .await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("database initialised");
    Ok(pool)
}
