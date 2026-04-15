use bot_council::build_app;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app = build_app().await?;
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3100").await?;
    tracing::info!("Bot Council listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
