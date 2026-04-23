use crate::bot_client::{DebateRoundRequest, RoundContext};
use crate::config::ModelsConfig;
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

/// Run Round 4: final position with position_change declaration.
///
/// Uses the shared retry-then-carry-forward dispatch helper. R4 has no
/// structural validation closure — `position_change` for text_only bots
/// is extracted from prose post-response, not required on the wire. On
/// HTTP failure, timeout, or stock-abstention text the bot gets one
/// simplified retry; if that also fails the bot's Round 0 text is
/// carried forward so its voice is preserved. Only when Round 0 itself
/// was unavailable does the row mark a genuine abstention for R4.
/// `responses.retry_count` and `responses.fallback_from_round` capture
/// the outcome for analytics.
///
/// Post-round extraction runs for text_only bots whose prose response
/// lacks a structured `position_change` field; see
/// `crate::orchestrator::extraction::extract_if_needed`. Extraction
/// only runs on the Success arm — carried-forward R0 text is not a
/// final-position answer, so running extraction against it would lie.
#[allow(clippy::too_many_arguments)]
pub async fn run_round4(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    full_context: Vec<RoundContext>,
    models_config: &ModelsConfig,
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
            let prompt = prompts::round4_prompt(&topic);
            let retry_prompt = simplified_retry_prompt(&topic, 4);
            let context = full_context.clone();
            let r0_text = r0_by_bot.get(&bot.id).cloned();
            let bot_id = bot.id.clone();
            async move {
                let req = DebateRoundRequest {
                    session_id,
                    round: 4,
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
                    |_| false, // no structural validation in R4
                )
                .await;
                (bot_id, bot_kind, outcome)
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    // Iterate sequentially because the Success arm may hit MiniMax for
    // text_only extraction; per-round bot count is small (typically 5).
    for (bot_id, bot_kind, outcome) in results {
        let (
            response_text,
            confidence,
            abstained,
            retry_count,
            fallback_from_round,
            position_change_json,
            extraction_metadata_json,
        ) = match outcome {
            DispatchOutcome::Success {
                mut response,
                retry_count,
            } => {
                response_parser::normalise_response(&mut response);
                // Post-round extraction for text_only bots whose prose
                // response lacks a structured position_change field.
                // No-op for external bots (short-circuits on bot_kind).
                let provenance = crate::orchestrator::extraction::extract_if_needed(
                    models_config,
                    &bot_kind,
                    crate::extractor::ExtractTarget::PositionChange,
                    &mut response,
                )
                .await;
                let extraction_metadata_json = serde_json::to_string(&serde_json::json!({
                    "position_change": provenance.to_json()
                }))
                .ok();
                let position_change_json = response
                    .position_change
                    .as_ref()
                    .and_then(|pc| serde_json::to_string(pc).ok());
                (
                    response.response,
                    response.confidence,
                    false,
                    retry_count as i64,
                    None,
                    position_change_json,
                    extraction_metadata_json,
                )
            }
            DispatchOutcome::CarriedForward {
                r0_text,
                retry_count,
            } => (
                r0_text,
                None,
                false,
                retry_count as i64,
                Some(0i64),
                None,
                None,
            ),
            DispatchOutcome::Abstained { retry_count } => (
                "(abstained)".to_string(),
                None,
                true,
                retry_count as i64,
                None,
                None,
                None,
            ),
        };
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool,
            &resp_id,
            debate_id,
            4,
            &bot_id,
            &response_text,
            confidence,
            None,
            position_change_json.as_deref(),
            true,
            retry_count,
            abstained,
            extraction_metadata_json.as_deref(),
            fallback_from_round,
        )
        .await
        .map_err(|e| format!("db error storing Round 4 response: {e}"))?;
    }

    Ok(())
}
