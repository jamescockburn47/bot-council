//! Re-run the final synthesis for one or many completed debates using the
//! current model routing. Rebuilds every input (participant map,
//! transcript lines, precomputed summary, stored divergence analyses,
//! grounding evidence) from the database alone — no orchestrator state
//! required — and invokes `synthesiser::run_synthesis` with whatever
//! model is configured NOW. The old `syntheses` row is replaced with the
//! new one in a single transaction.
//!
//! Triggered from the CLI (`bot-council resynthesise` or
//! `bot-council resynthesise <debate-id>`). Throttles between debates so
//! we don't hit the MiniMax (or any other paid) rate limit on a bulk run.

use crate::config::Settings;
use crate::db;
use crate::db::queries;
use crate::db::queries_phase1;
use crate::orchestrator::multi_round::is_effective_abstention_response;
use crate::synthesiser;
use crate::synthesiser::{citation_check, precompute};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

/// How long to wait between successive resyntheses. Conservative default;
/// override from the CLI (`--throttle-ms`) if the plan's rate limit is
/// known to be higher.
const DEFAULT_THROTTLE_MS: u64 = 2000;

#[derive(Debug, Default)]
pub struct ResynthReport {
    pub considered: usize,
    pub succeeded: usize,
    pub failed: Vec<(String, String)>,
    pub skipped: usize,
}

/// Run resynthesis across the debate list.
///
/// * `only_id` — when set, only that debate is processed.
/// * `throttle_ms` — milliseconds to sleep between successive resyntheses
///   (ignored for `only_id` mode).
pub async fn resynth(
    settings: &Settings,
    only_id: Option<&str>,
    throttle_ms: Option<u64>,
) -> Result<ResynthReport> {
    let pool = db::init_pool(&settings.database.url)
        .await
        .context("open db for resynth")?;

    let ids = match only_id {
        Some(id) => vec![id.to_string()],
        None => target_debate_ids(&pool)
            .await
            .context("find target debates")?,
    };
    let delay = throttle_ms.unwrap_or(DEFAULT_THROTTLE_MS);
    let mut report = ResynthReport {
        considered: ids.len(),
        ..Default::default()
    };

    for (i, id) in ids.iter().enumerate() {
        if i > 0 && only_id.is_none() && delay > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
        match resynth_one(&pool, &settings.models, &settings.debate, id).await {
            Ok(ResynthOutcome::Rewritten) => {
                report.succeeded += 1;
                tracing::info!(debate_id = %id, "resynth: rewritten");
            }
            Ok(ResynthOutcome::SkippedNoTranscript) => {
                report.skipped += 1;
                tracing::warn!(debate_id = %id, "resynth: skipped — no transcript");
            }
            Err(e) => {
                let msg = format!("{e:#}");
                report.failed.push((id.clone(), msg.clone()));
                tracing::warn!(debate_id = %id, error = %msg, "resynth: failed");
            }
        }
    }

    tracing::info!(
        considered = report.considered,
        succeeded = report.succeeded,
        skipped = report.skipped,
        failed = report.failed.len(),
        "resynth: done"
    );
    Ok(report)
}

enum ResynthOutcome {
    Rewritten,
    SkippedNoTranscript,
}

/// Eligible for automatic rerun: terminal status, not archived, not a
/// non-production topic (the same marker list the cleanup job uses).
async fn target_debate_ids(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    // We fetch all debates and filter in code; the list is small enough
    // that issuing the topic filter as SQL isn't worth the string
    // plumbing (queries.rs `list_debates` already handles that for the
    // list endpoint — we don't reuse it because we need ids only).
    let rows = queries::list_debates(pool, None, 10_000, false, false).await?;
    Ok(rows
        .into_iter()
        .filter(|r| matches!(r.status.as_str(), "complete" | "failed"))
        .map(|r| r.id)
        .collect())
}

async fn resynth_one(
    pool: &SqlitePool,
    models_config: &crate::config::ModelsConfig,
    debate_config: &crate::config::DebateConfig,
    debate_id: &str,
) -> Result<ResynthOutcome> {
    let debate = queries::get_debate(pool, debate_id)
        .await?
        .with_context(|| format!("debate {debate_id} not found"))?;

    let all_responses = queries_phase1::get_all_responses(pool, debate_id).await?;
    if all_responses.is_empty() {
        return Ok(ResynthOutcome::SkippedNoTranscript);
    }

    let debate_bots = queries_phase1::get_debate_bots_with_roles(pool, debate_id).await?;
    let bots = queries::get_any_bots_by_ids(
        pool,
        &debate_bots
            .iter()
            .map(|db| db.bot_id.clone())
            .collect::<Vec<_>>(),
    )
    .await?;
    let bot_name_by_id: HashMap<String, String> = bots
        .iter()
        .map(|b| (b.id.clone(), b.name.clone()))
        .collect();
    let pseudonym_map: HashMap<String, String> = debate_bots
        .iter()
        .map(|db| (db.bot_id.clone(), db.pseudonym.clone()))
        .collect();

    let participant_map_text = debate_bots
        .iter()
        .map(|db| {
            let bot_name = bot_name_by_id
                .get(&db.bot_id)
                .cloned()
                .unwrap_or_else(|| db.bot_id.clone());
            format!("{} = {}", db.pseudonym, bot_name)
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Transcript lines + grounding rows (same construction as the
    // orchestrator's inline block — duplication accepted for
    // standalone-ability).
    let mut transcript_lines: Vec<String> = Vec::new();
    let mut grounding_rows: Vec<serde_json::Value> = Vec::new();
    for resp in &all_responses {
        let pseudo = pseudonym_map.get(&resp.bot_id).cloned().unwrap_or_default();
        let effective_abstained = is_effective_abstention_response(&resp.response_json);
        grounding_rows.push(serde_json::json!({
            "agent": pseudo,
            "round": resp.round_number,
            "abstained": resp.abstained,
            "effective_abstained": effective_abstained,
            "valid": resp.valid,
            "response": resp.response_json,
        }));
        if resp.abstained || effective_abstained {
            continue;
        }
        let mut lines = resp.response_json.lines();
        let first_line = lines.next().unwrap_or_default();
        let mut sanitized_response = first_line.to_string();
        for line in lines {
            sanitized_response.push('\n');
            sanitized_response.push_str("  ");
            sanitized_response.push_str(line);
        }
        transcript_lines.push(format!(
            "[{pseudo}, Round {}]: {}",
            resp.round_number, sanitized_response
        ));
    }

    let precomputed = precompute::precompute(&all_responses, &pseudonym_map);
    let precomputed_json = serde_json::to_string(&precomputed).unwrap_or_default();

    // Rebuild divergence input from the stored `analyses` rows. If none
    // exist (older debates with no divergence pass) we pass an empty
    // list — the synthesis prompt handles that gracefully.
    let divergence_rows = queries_phase1::get_analyses(pool, debate_id, Some("divergence")).await?;
    let bot_to_pseudo = &pseudonym_map;
    let div_json: Vec<serde_json::Value> = divergence_rows
        .iter()
        .filter_map(|row| {
            let bot_id = row.bot_id.as_ref()?;
            let pseudo = bot_to_pseudo.get(bot_id)?.clone();
            let analysis = serde_json::from_str::<serde_json::Value>(&row.result_json).ok()?;
            Some(serde_json::json!({ "pseudonym": pseudo, "analysis": analysis }))
        })
        .collect();
    let divergence_json = serde_json::to_string(&div_json).unwrap_or("[]".to_string());
    let grounding_evidence_json = serde_json::to_string(&grounding_rows).unwrap_or_default();

    // Rebuild the selected crux (if any) from the stored `crux_selection`
    // analysis row. Older debates pre-date crux selection and simply have
    // no such row — `None` flows through and the synthesis prompt omits
    // the crux-outcome section.
    let crux_rows = queries_phase1::get_analyses(pool, debate_id, Some("crux_selection")).await?;
    let crux: Option<crate::analyser::crux::CruxSelection> = crux_rows
        .iter()
        .find_map(|row| serde_json::from_str(&row.result_json).ok());

    // Call the synthesiser with whatever the running process has for
    // model routing. Warmup intentionally skipped — MiniMax doesn't need
    // it and we've disabled it in env anyway.
    let (synthesis_output, prompt_hash) = synthesiser::run_synthesis(
        models_config,
        &debate.topic,
        &participant_map_text,
        &transcript_lines.join("\n\n"),
        &precomputed_json,
        &divergence_json,
        &grounding_evidence_json,
        crux.as_ref(),
        debate_config.synthesis_temperature,
    )
    .await
    .map_err(|e| anyhow::anyhow!("synthesis call failed: {e}"))?;

    // Stash the input hash distinct from the library's own — useful for
    // debugging two prompt versions against the same debate.
    let _ = prompt_hash_for_inputs(
        &debate.topic,
        &participant_map_text,
        &transcript_lines.join("\n\n"),
    );

    // Citation check against the new output.
    let synthesis_value: serde_json::Value = serde_json::from_str(&synthesis_output)
        .with_context(|| "parse synthesis output for citation check")?;
    let valid_pseudonyms: HashSet<String> = pseudonym_map.values().cloned().collect();
    let final_round_number = all_responses
        .iter()
        .map(|r| r.round_number)
        .max()
        .unwrap_or(0);
    let responses_by_pseudonym_round: HashMap<(String, i64), bool> = all_responses
        .iter()
        .filter_map(|r| {
            pseudonym_map.get(&r.bot_id).cloned().map(|pseudo| {
                (
                    (pseudo, r.round_number),
                    r.abstained || is_effective_abstention_response(&r.response_json),
                )
            })
        })
        .collect();
    let citation_result = citation_check::check_citations(
        &synthesis_value,
        &valid_pseudonyms,
        &responses_by_pseudonym_round,
        final_round_number,
    );
    let citation_json =
        serde_json::to_string(&citation_result).unwrap_or_else(|_| "{}".to_string());

    // Replace the existing syntheses row. DELETE+INSERT in a transaction
    // is simpler (and SQLite-portable) than crafting a SQLite UPSERT
    // that matches the full column set.
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM syntheses WHERE debate_id = ?")
        .bind(debate_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO syntheses (debate_id, output_json, model_used, prompt_hash, citation_check_json) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(debate_id)
    .bind(&synthesis_output)
    .bind(models_config.effective_final_synthesis_model())
    .bind(&prompt_hash)
    .bind(&citation_json)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(ResynthOutcome::Rewritten)
}

fn prompt_hash_for_inputs(topic: &str, participants: &str, transcript: &str) -> String {
    let mut h = Sha256::new();
    h.update(topic.as_bytes());
    h.update(b"\0");
    h.update(participants.as_bytes());
    h.update(b"\0");
    h.update(transcript.as_bytes());
    hex::encode(h.finalize())
}
