use crate::bot_client::{self, DebateRoundRequest, DebateRoundResponse, RoundContext};
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::orchestrator::{prompts, response_parser};
use crate::types::Role;
use reqwest_middleware::ClientWithMiddleware;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Dispatch Round 1 (anonymous distribution) to all bots with anonymised Round 0 context.
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
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
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
            let context = round0_context.clone();
            let bot_id = bot.id.clone();
            async move {
                let req = DebateRoundRequest {
                    session_id,
                    round: 1,
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
                        tracing::warn!(bot_id = %bot_id, error = %e, "Round 1: bot request failed");
                        (bot_id, None)
                    }
                    Err(_) => {
                        tracing::warn!(bot_id = %bot_id, "Round 1: bot request timed out");
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
    for (bot_id, resp_opt) in &results {
        let (response_text, confidence, abstained) = match resp_opt {
            Some(r) => (r.response.clone(), r.confidence, false),
            None => ("(abstained)".to_string(), None, true),
        };
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool,
            &resp_id,
            debate_id,
            1,
            bot_id,
            &response_text,
            confidence,
            None,
            None,
            true,
            0,
            abstained,
            None,
            None,
        )
        .await
        .map_err(|e| format!("db error storing Round 1 response: {e}"))?;
    }

    Ok(results)
}
