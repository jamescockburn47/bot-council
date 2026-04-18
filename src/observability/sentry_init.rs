//! Sentry initialisation. Returns `Some(guard)` when the DSN is set, `None`
//! otherwise. The guard must be kept alive for the lifetime of the process
//! so the Sentry client can flush events at shutdown.

use crate::config::SentryConfig;
use std::sync::Arc;

/// Initialise Sentry if `config.dsn` is non-empty. Returns the Sentry client
/// guard on success, or `None` if Sentry is disabled or the DSN is invalid.
/// On invalid DSN, logs a warning via `tracing` (which is why this must be
/// called AFTER the tracing subscriber is installed — unlike plain panic
/// capture which sentry::init patches in synchronously).
///
/// Scrubbing: the `before_send` and `before_breadcrumb` hooks in
/// [`crate::observability::scrubber`] remove sensitive fields (bearer
/// tokens, JWTs, bot token ciphertext) before events leave the process.
pub fn init_sentry(config: &SentryConfig) -> Option<sentry::ClientInitGuard> {
    if config.dsn.trim().is_empty() {
        tracing::info!("sentry disabled: APP__SENTRY__DSN not set");
        return None;
    }

    // Prefer explicit SENTRY_RELEASE (git SHA from deploy pipeline); fall back
    // to Cargo package name@version via sentry::release_name!().
    let release = std::env::var("SENTRY_RELEASE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(std::borrow::Cow::Owned)
        .or_else(|| sentry::release_name!());

    let guard = sentry::init((
        config.dsn.clone(),
        sentry::ClientOptions {
            release: release.clone(),
            environment: Some(config.environment.clone().into()),
            traces_sample_rate: config.traces_sample_rate,
            sample_rate: 1.0,
            attach_stacktrace: true,
            before_send: Some(Arc::new(|event| {
                crate::observability::scrubber::before_send(event)
            })),
            before_breadcrumb: Some(Arc::new(|bc| {
                crate::observability::scrubber::before_breadcrumb(bc)
            })),
            ..Default::default()
        },
    ));

    tracing::info!(
        environment = %config.environment,
        traces_sample_rate = config.traces_sample_rate,
        release = %release.as_deref().unwrap_or("<unset>"),
        "sentry initialised"
    );
    Some(guard)
}
