use crate::bot_client::{self, DebateRoundRequest, DebateRoundResponse, RoundContext};
use crate::config::ModelsConfig;
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::orchestrator::{prompts, response_parser};
use crate::types::Role;
use reqwest_middleware::ClientWithMiddleware;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Run Round 4: final position with position_change declaration.
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
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    let prompt = prompts::round4_prompt(topic);

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
            let context = full_context.clone();
            let bot_id = bot.id.clone();
            async move {
                let req = DebateRoundRequest {
                    session_id,
                    round: 4,
                    role: role.as_str().to_string(),
                    context,
                    prompt,
                };
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    bot_client::dispatch_round_request(&client, &bot_kind, &endpoint, &token, &req),
                )
                .await;
                match result {
                    Ok(Ok(resp)) => (bot_id, Some(resp)),
                    Ok(Err(e)) => {
                        tracing::warn!(bot_id = %bot_id, error = %e, "Round 4: bot request failed");
                        (bot_id, None)
                    }
                    Err(_) => {
                        tracing::warn!(bot_id = %bot_id, "Round 4: bot request timed out");
                        (bot_id, None)
                    }
                }
            }
        })
        .collect();

    let mut results = futures::future::join_all(futures).await;

    for (_, resp_opt) in &mut results {
        if let Some(r) = resp_opt {
            response_parser::normalise_response(r);
        }
    }
    // Persist each response. For non-abstained text_only bots whose
    // response lacks a structured position_change field, run post-round
    // extraction first so the patched field + provenance land together in
    // the DB. External bots short-circuit inside `extract_if_needed`.
    // Iteration is sequential because each extraction call may hit MiniMax;
    // the per-round bot count is small (typically 5).
    for (bot_id, resp_opt) in &mut results {
        let (response_text, confidence, pc_json, abstained, extraction_metadata_json) =
            match resp_opt {
                Some(resp) => {
                    let bot_kind = bots
                        .iter()
                        .find(|b| &b.id == bot_id)
                        .map(|b| b.bot_kind.as_str())
                        .unwrap_or("external")
                        .to_string();
                    let provenance = crate::orchestrator::extraction::extract_if_needed(
                        models_config,
                        &bot_kind,
                        crate::extractor::ExtractTarget::PositionChange,
                        resp,
                    )
                    .await;
                    let meta = serde_json::to_string(&serde_json::json!({
                        "position_change": provenance.to_json()
                    }))
                    .ok();
                    let pc = resp
                        .position_change
                        .as_ref()
                        .and_then(|pc| serde_json::to_string(pc).ok());
                    (resp.response.clone(), resp.confidence, pc, false, meta)
                }
                None => ("(abstained)".to_string(), None, None, true, None),
            };
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool,
            &resp_id,
            debate_id,
            4,
            bot_id,
            &response_text,
            confidence,
            None,
            pc_json.as_deref(),
            true,
            0,
            abstained,
            extraction_metadata_json.as_deref(),
        )
        .await
        .map_err(|e| format!("db error storing Round 4 response: {e}"))?;
    }

    Ok(results)
}
