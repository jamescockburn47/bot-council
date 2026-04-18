use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{self, DebateRoundRequest, DebateRoundResponse};
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::orchestrator::{error_kind, prompts, response_parser};
use crate::types::Role;

/// Dispatch Round 0 (blind formation) to all bots concurrently.
/// Each bot receives the topic and its assigned role with no prior context.
pub async fn run_round0(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    timeout_secs: u64,
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    let futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = debate_id.to_string();
        let role = role_assignments.get(&bot.id).copied().unwrap_or(Role::Proponent);
        let prompt = prompts::round0_prompt(topic, role);
        let bot_id = bot.id.clone();
        async move {
            let req = DebateRoundRequest {
                session_id,
                round: 0,
                role: role.as_str().to_string(),
                context: vec![],
                prompt,
            };
            let start = std::time::Instant::now();
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                bot_client::send_debate_request(&client, &endpoint, &token, &req),
            ).await;
            let elapsed_ms = start.elapsed().as_millis() as i64;
            match result {
                Ok(Ok(resp)) => (bot_id, Some(resp), elapsed_ms, None),
                Ok(Err(e)) => {
                    let c = error_kind::from_client_error(&e);
                    tracing::warn!(
                        bot_id = %bot_id, error = %e, kind = %c.kind,
                        "Round 0: bot request failed"
                    );
                    (bot_id, None, elapsed_ms, Some(c))
                }
                Err(_) => {
                    let c = error_kind::from_timeout(timeout_secs);
                    tracing::warn!(
                        bot_id = %bot_id, kind = %c.kind,
                        "Round 0: bot request timed out"
                    );
                    (bot_id, None, elapsed_ms, Some(c))
                }
            }
        }
    }).collect();

    let mut results = futures::future::join_all(futures).await;

    // Normalise and store responses
    for (_, resp_opt, _, _) in &mut results {
        if let Some(r) = resp_opt {
            response_parser::normalise_response(r);
        }
    }
    for (bot_id, resp_opt, elapsed_ms, classification) in &results {
        let (response_text, abstained) = match resp_opt {
            Some(r) => (r.response.clone(), false),
            None => ("(abstained)".to_string(), true),
        };
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool, &resp_id, debate_id, 0, bot_id, &response_text,
            None, None, None, true, 0, abstained,
        ).await.map_err(|e| format!("db error storing Round 0 response: {e}"))?;
        if let Some(c) = classification {
            if let Err(e) = queries_phase1::update_response_error(
                pool, &resp_id, c.kind, &c.detail, *elapsed_ms,
            ).await {
                tracing::warn!(
                    bot_id = %bot_id, error = %e,
                    "Round 0: failed to record error classification"
                );
            }
        }
    }

    let stripped: Vec<(String, Option<DebateRoundResponse>)> = results
        .into_iter()
        .map(|(id, resp, _, _)| (id, resp))
        .collect();
    Ok(stripped)
}
