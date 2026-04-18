use bot_council::{build_app, config::Settings, observability};
use tracing_subscriber::{EnvFilter, prelude::*};

/// Entry point. Sentry is initialised BEFORE the Tokio runtime starts,
/// per sentry-rust 0.47 axum guidance (the SDK's background transport
/// thread must outlive the runtime, which `#[tokio::main]` can't
/// guarantee).
fn main() -> anyhow::Result<()> {
    let settings = Settings::load()?;
    settings.validate()?;

    // Sentry guard must live for the whole program. Dropping it during
    // shutdown flushes queued events.
    let _sentry_guard = observability::init_sentry(&settings.sentry);

    // Tracing subscriber with a Sentry bridge: ERROR/WARN → Sentry events,
    // INFO+ → breadcrumbs attached to any subsequent error.
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .with(sentry_tracing::layer())
        .init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let app = build_app().await?;
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3100").await?;
    tracing::info!("Bot Council listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
