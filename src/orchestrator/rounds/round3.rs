use crate::bot_client::{DebateRoundRequest, RoundContext};
use crate::config::ModelsConfig;
use crate::db::models::BotRow;
use crate::db::{queries, queries_phase1};
use crate::orchestrator::dispatch::{
    DispatchOutcome, dispatch_with_retry_and_fallback, simplified_retry_prompt,
};
use crate::orchestrator::rounds::round3_legacy::run_round3_cross_examination_legacy;
use crate::orchestrator::{prompts, response_parser};
use crate::types::Role;
use reqwest_middleware::ClientWithMiddleware;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Run Round 3. Dispatches the crux-engagement variant if the crux
/// selector produced a valid `CruxSelection` between R2 and R3; otherwise
/// falls back to the legacy two-pass cross-examination format so R3 still
/// runs when crux selection fails.
#[allow(clippy::too_many_arguments)]
pub async fn run_round3(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    pseudonym_map: &HashMap<String, String>,
    reverse_pseudonym_map: &HashMap<String, String>,
    round2_responses: &HashMap<String, String>,
    models_config: &ModelsConfig,
    crux: Option<&crate::analyser::crux::CruxSelection>,
    timeout_secs: u64,
) -> Result<(), String> {
    match crux {
        Some(c) => {
            run_round3_crux(
                pool,
                client,
                debate_id,
                topic,
                bots,
                bot_tokens,
                role_assignments,
                pseudonym_map,
                c,
                timeout_secs,
            )
            .await
        }
        None => {
            run_round3_cross_examination_legacy(
                pool,
                client,
                debate_id,
                topic,
                bots,
                bot_tokens,
                role_assignments,
                reverse_pseudonym_map,
                round2_responses,
                models_config,
                timeout_secs,
            )
            .await?;
            Ok(())
        }
    }
}

/// Crux-engagement R3: every bot receives the same claim + source quote
/// and engages it directly, with R0+R1+R2 provided as prior context.
///
/// Mirrors `run_round1`'s resilient dispatch: one simplified retry on
/// failure, R0-carry-forward if both attempts fail, genuine abstention
/// only if R0 itself is unavailable. Task 18 will layer a structured
/// `crux_engagement` extraction on top of this; for now we persist the
/// raw response without extraction metadata.
#[allow(clippy::too_many_arguments)]
async fn run_round3_crux(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    pseudonym_map: &HashMap<String, String>,
    crux: &crate::analyser::crux::CruxSelection,
    timeout_secs: u64,
) -> Result<(), String> {
    // Fetch each bot's non-abstained R0 text once for potential carry-forward.
    let r0_rows = queries::get_responses(pool, debate_id, 0)
        .await
        .map_err(|e| format!("db: {e}"))?;
    let r0_by_bot: HashMap<String, String> = r0_rows
        .iter()
        .filter(|r| !r.abstained)
        .map(|r| (r.bot_id.clone(), r.response_json.clone()))
        .collect();

    // Build prior-round context (R0 + R1 + R2) so the bot has full history
    // when engaging the crux. Mirrors the `full_context` assembly used for
    // R4 in multi_round.rs.
    let prior = queries_phase1::get_all_responses(pool, debate_id)
        .await
        .map_err(|e| format!("db: {e}"))?;
    let prior_context: Vec<RoundContext> = prior
        .iter()
        .filter(|r| !r.abstained && r.round_number <= 2)
        .map(|r| RoundContext {
            pseudonym: pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default(),
            round: r.round_number,
            response: r.response_json.clone(),
            confidence: r.confidence,
        })
        .collect();

    let prompt = prompts::round3_crux_prompt(
        topic,
        &crux.claim,
        &crux.source_pseudonym,
        &crux.source_quote,
    );
    let topic = topic.to_string();

    let futures: Vec<_> = bots
        .iter()
        .map(|bot| {
            let client = client.clone();
            let endpoint = bot.endpoint_url.clone();
            let bot_kind = bot.bot_kind.clone();
            let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
            let session_id = debate_id.to_string();
            let role = role_assignments
                .get(&bot.id)
                .copied()
                .unwrap_or(Role::Proponent);
            let prompt = prompt.clone();
            let retry_prompt = simplified_retry_prompt(&topic, 3);
            let context = prior_context.clone();
            let r0_text = r0_by_bot.get(&bot.id).cloned();
            let bot_id = bot.id.clone();
            async move {
                let req = DebateRoundRequest {
                    session_id,
                    round: 3,
                    role: role.as_str().to_string(),
                    context,
                    prompt,
                };
                let outcome = dispatch_with_retry_and_fallback(
                    &client,
                    &bot_kind,
                    &endpoint,
                    &token,
                    &req,
                    retry_prompt,
                    r0_text,
                    timeout_secs,
                    |_| false, // no structural validation for R3 crux
                )
                .await;
                (bot_id, outcome)
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    for (bot_id, outcome) in results {
        let (response_text, confidence, abstained, retry_count, fallback_from_round) =
            match outcome {
                DispatchOutcome::Success {
                    mut response,
                    retry_count,
                } => {
                    response_parser::normalise_response(&mut response);
                    (
                        response.response,
                        response.confidence,
                        false,
                        retry_count as i64,
                        None,
                    )
                }
                DispatchOutcome::CarriedForward {
                    r0_text,
                    retry_count,
                } => (r0_text, None, false, retry_count as i64, Some(0i64)),
                DispatchOutcome::Abstained { retry_count } => (
                    "(abstained)".to_string(),
                    None,
                    true,
                    retry_count as i64,
                    None,
                ),
            };
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool,
            &resp_id,
            debate_id,
            3,
            &bot_id,
            &response_text,
            confidence,
            None,
            None,
            true,
            retry_count,
            abstained,
            None,
            fallback_from_round,
        )
        .await
        .map_err(|e| format!("db error storing Round 3 crux response: {e}"))?;
    }

    Ok(())
}
