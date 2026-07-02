//! Ship's-log writers for the debate pipeline: the journal calls the
//! orchestrator and resynth CLI make, kept out of `multi_round.rs` so the
//! round logic stays about rounds (and inside its file-length ceiling).
//! All writers are fire-and-forget via `events::record_event`.

use crate::observability::events::{self, EventScope};
use sqlx::SqlitePool;

/// True when synthesis contains no claim-bearing sections.
pub(crate) fn is_conservative_empty_synthesis(s: &serde_json::Value) -> bool {
    s.get("issues")
        .and_then(|v| v.as_array())
        .map(|a| a.is_empty())
        .unwrap_or(false)
}

/// A debate could not start: too few debaters answered the opening round.
pub(crate) async fn record_quorum_failure(
    pool: &SqlitePool,
    debate_id: &str,
    topic: &str,
    active: usize,
    reason: &str,
) {
    events::record_event(
        pool,
        "quorum_not_met",
        EventScope {
            label: &format!("Debate \"{topic}\""),
            debate_id: Some(debate_id),
            bot_id: None,
        },
        &format!("Only {active} debaters answered the opening round."),
        Some(serde_json::json!({"reason": reason})),
    )
    .await;
}

/// The rounds finished but the summariser could not produce the analysis.
pub(crate) async fn record_synthesis_failure(pool: &SqlitePool, debate_id: &str, reason: &str) {
    events::record_event(
        pool,
        "debate_failed",
        EventScope {
            label: &debate_label(debate_id),
            debate_id: Some(debate_id),
            bot_id: None,
        },
        "The rounds completed, but the summariser could not produce the analysis.",
        Some(serde_json::json!({"error": reason})),
    )
    .await;
}

/// Post-store quality events: fallback synthesis and artifact-level
/// sentinel violations (checked here because the pool lives here).
pub(crate) async fn record_synthesis_quality(
    pool: &SqlitePool,
    debate_id: &str,
    synthesis_value: &serde_json::Value,
    crux_selected: bool,
) {
    let label = debate_label(debate_id);
    if is_conservative_empty_synthesis(synthesis_value) {
        events::record_event(
            pool,
            "synthesis_fallback",
            EventScope {
                label: &label,
                debate_id: Some(debate_id),
                bot_id: None,
            },
            "",
            None,
        )
        .await;
    }
    if let Ok(artifact) = serde_json::from_value::<crate::synthesiser::schema::SessionArtifact>(
        synthesis_value.clone(),
    ) {
        for v in crate::observability::sentinels::check_artifact(&artifact, crux_selected) {
            events::record_event(
                pool,
                "sentinel_violation",
                EventScope {
                    label: &label,
                    debate_id: Some(debate_id),
                    bot_id: None,
                },
                &format!("Self-check {}: {}.", v.sentinel_id, v.detail),
                Some(serde_json::json!({"sentinel": v.sentinel_id, "detail": v.detail})),
            )
            .await;
        }
    }
}

/// A resynth batch finished.
pub(crate) async fn record_resynth_run(
    pool: &SqlitePool,
    succeeded: usize,
    skipped: usize,
    failed: usize,
) {
    events::record_event(
        pool,
        "resynth_run",
        EventScope::default(),
        &format!(
            "Summaries were rebuilt for {succeeded} debates; {skipped} skipped; {failed} failed."
        ),
        None,
    )
    .await;
}

fn debate_label(debate_id: &str) -> String {
    format!(
        "the debate {}",
        debate_id.chars().take(8).collect::<String>()
    )
}
