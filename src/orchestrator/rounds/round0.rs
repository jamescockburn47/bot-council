use crate::bot_client::{self, DebateRoundRequest, DebateRoundResponse};
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::orchestrator::{prompts, response_parser};
use crate::types::Role;
use reqwest_middleware::ClientWithMiddleware;
use sqlx::SqlitePool;
use std::collections::HashMap;

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
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    bot_client::dispatch_round_request(&client, &bot_kind, &endpoint, &token, &req),
                )
                .await;
                match result {
                    Ok(Ok(resp)) => (bot_id, Some(resp)),
                    Ok(Err(e)) => {
                        tracing::warn!(bot_id = %bot_id, error = %e, "Round 0: bot request failed");
                        (bot_id, None)
                    }
                    Err(_) => {
                        tracing::warn!(bot_id = %bot_id, "Round 0: bot request timed out");
                        (bot_id, None)
                    }
                }
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    // Normalise and store responses
    let mut results: Vec<(String, Option<DebateRoundResponse>)> = results;
    for (_, resp_opt) in &mut results {
        if let Some(r) = resp_opt {
            response_parser::normalise_response(r);
        }
    }
    for (bot_id, resp_opt) in &results {
        let (response_text, abstained) = match resp_opt {
            Some(r) => (r.response.clone(), false),
            None => ("(abstained)".to_string(), true),
        };
        let ingest_kind = resp_opt.as_ref().map(|r| r.ingest_kind.as_str());
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool,
            &resp_id,
            debate_id,
            0,
            bot_id,
            &response_text,
            None,
            None,
            None,
            true,
            0,
            abstained,
            None,
            None,
            ingest_kind,
        )
        .await
        .map_err(|e| format!("db error storing Round 0 response: {e}"))?;
    }

    Ok(results)
}
