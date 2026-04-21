//! Scheduled test-debate cleanup.
//!
//! Deletes debates whose topic matches `NON_PRODUCTION_TOPIC_MARKERS`
//! (operator probes, smoke tests, verification runs, etc.) so the debate
//! list stays focused on real work. Triggered by the `test-cleanup`
//! subcommand of the `bot-council` binary, run from the
//! `bot-council-test-cleanup.timer` systemd unit.
//!
//! Safe by construction:
//! * only touches debates in terminal status (`complete`/`cancelled`/`failed`)
//! * default grace window of one hour so a freshly-run probe can still
//!   be inspected before it disappears
//! * cascades to child rows in a single transaction per debate
//! * no-op when nothing matches

use crate::config::Settings;
use crate::db;
use crate::db::queries_cleanup::{cascade_delete_debate, find_stale_test_debate_ids};
use anyhow::Context;

const DEFAULT_GRACE_HOURS: i64 = 1;

/// Run the cleanup and return the count of deleted debates. Per-row errors
/// are logged but don't abort the batch.
pub async fn run_test_cleanup(settings: &Settings) -> anyhow::Result<usize> {
    let pool = db::init_pool(&settings.database.url)
        .await
        .context("open db for test-cleanup")?;
    let ids = find_stale_test_debate_ids(&pool, DEFAULT_GRACE_HOURS)
        .await
        .context("find stale test debates")?;

    if ids.is_empty() {
        tracing::info!("test-cleanup: nothing to delete");
        return Ok(0);
    }

    let mut deleted = 0usize;
    for id in &ids {
        match cascade_delete_debate(&pool, id).await {
            Ok(()) => {
                deleted += 1;
                tracing::info!(debate_id = %id, "test-cleanup: deleted");
            }
            Err(e) => {
                tracing::warn!(debate_id = %id, error = %e, "test-cleanup: delete failed");
            }
        }
    }
    tracing::info!(deleted, considered = ids.len(), "test-cleanup: done");
    Ok(deleted)
}
