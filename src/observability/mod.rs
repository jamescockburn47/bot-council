//! Observability integration: Sentry error tracking and PII scrubbing.
//!
//! The public entry point is [`init_sentry`] called once from `main`, before
//! `tracing_subscriber` so that the `sentry_tracing::layer()` in `lib.rs`
//! has a live Sentry client to talk to.

pub mod events;
pub mod scrubber;
pub mod sentinels;
pub mod sentry_init;
pub mod system_guidance;

pub use sentry_init::init_sentry;
