use bot_council::{build_app, config::Settings, observability};
use tracing_subscriber::{EnvFilter, prelude::*};

/// Entry point. Sentry is initialised BEFORE the Tokio runtime starts,
/// per sentry-rust 0.47 axum guidance (the SDK's background transport
/// thread must outlive the runtime, which `#[tokio::main]` can't
/// guarantee).
fn main() -> anyhow::Result<()> {
    let command = std::env::args().nth(1);
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
    match command.as_deref() {
        Some("scoreboard-weekly") => runtime.block_on(async_scoreboard_weekly(settings)),
        Some("test-cleanup") => runtime.block_on(async_test_cleanup(settings)),
        Some("resynthesise") | Some("resynthesize") => {
            let args: Vec<String> = std::env::args().skip(2).collect();
            runtime.block_on(async_resynth(settings, args))
        }
        _ => runtime.block_on(async_main()),
    }
}

async fn async_main() -> anyhow::Result<()> {
    let app = build_app().await?;
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3100").await?;
    tracing::info!("Bot Council listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn async_scoreboard_weekly(settings: Settings) -> anyhow::Result<()> {
    let count = bot_council::scoreboard::run_weekly_snapshot(&settings).await?;
    tracing::info!(snapshot_count = count, "weekly scoreboard job finished");
    Ok(())
}

async fn async_test_cleanup(settings: Settings) -> anyhow::Result<()> {
    let deleted = bot_council::cleanup::run_test_cleanup(&settings).await?;
    tracing::info!(deleted, "test-cleanup job finished");
    Ok(())
}

/// CLI: `bot-council resynthesise [<debate_id>] [--throttle-ms N]`
///
/// No positional arg → resynth every completed/failed debate
/// (non-archived, production topic), throttled at 2s between calls.
/// A positional arg treats it as a single debate id (bypasses throttle).
async fn async_resynth(settings: Settings, args: Vec<String>) -> anyhow::Result<()> {
    let mut only_id: Option<String> = None;
    let mut throttle_ms: Option<u64> = None;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--throttle-ms" {
            if let Some(v) = args.get(i + 1) {
                throttle_ms = v.parse().ok();
                i += 2;
                continue;
            }
        }
        // First non-flag positional is the debate id.
        if only_id.is_none() && !a.starts_with("--") {
            only_id = Some(a.clone());
        }
        i += 1;
    }

    let report = bot_council::resynth::resynth(&settings, only_id.as_deref(), throttle_ms).await?;
    tracing::info!(
        considered = report.considered,
        succeeded = report.succeeded,
        skipped = report.skipped,
        failed = report.failed.len(),
        "resynth job finished"
    );
    for (id, msg) in &report.failed {
        tracing::warn!(debate_id = %id, error = %msg, "resynth failure detail");
    }
    Ok(())
}
