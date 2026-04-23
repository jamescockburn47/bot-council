use crate::bot_client::{DebateRoundRequest, RoundContext};
use crate::db::models::BotRow;
use crate::db::{queries, queries_phase1};
use crate::orchestrator::dispatch::{
    DispatchOutcome, dispatch_with_retry_and_fallback, simplified_retry_prompt,
};
use crate::orchestrator::{prompts, response_parser};
use crate::types::Role;
use reqwest_middleware::ClientWithMiddleware;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Dispatch Round 1 (anonymous distribution) to all bots with anonymised Round 0 context.
///
/// Uses the dispatch helper's retry-then-carry-forward resilience path:
/// on HTTP failure, timeout, or stock-abstention text, the bot gets one
/// simplified retry. If the retry also fails, the bot's Round 0 text is
/// carried forward so its voice is preserved for the rest of the debate.
/// Only if Round 0 itself is unavailable does Round 1 mark a genuine
/// abstention. `responses.retry_count` and `responses.fallback_from_round`
/// capture the outcome for downstream analytics.
#[allow(clippy::too_many_arguments)]
pub async fn run_round1(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    pseudonym_map: &HashMap<String, String>,
    round0_context: Vec<RoundContext>,
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
            let own_pseudonym = pseudonym_map.get(&bot.id).cloned().unwrap_or_default();
            let prompt = prompts::round1_prompt(&topic, &own_pseudonym, role);
            let retry_prompt = simplified_retry_prompt(&topic, 1);
            let context = round0_context.clone();
            let r0_text = r0_by_bot.get(&bot.id).cloned();
            let bot_id = bot.id.clone();
            async move {
                let req = DebateRoundRequest {
                    session_id,
                    round: 1,
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
                    |_| false, // no structural validation in R1
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
            1,
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
        .map_err(|e| format!("db error storing Round 1 response: {e}"))?;
    }

    Ok(())
}
