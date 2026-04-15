use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{self, DebateRoundRequest, RoundContext, DebateRoundResponse};
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::orchestrator::prompts;
use crate::types::Role;

/// Run Round 4: final position with position_change declaration.
pub async fn run_round4(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    full_context: Vec<RoundContext>,
    timeout_secs: u64,
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    let prompt = prompts::round4_prompt(topic);

    let futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = debate_id.to_string();
        let role = role_assignments.get(&bot.id).copied().unwrap_or(Role::Proponent);
        let prompt = prompt.clone();
        let context = full_context.clone();
        let bot_id = bot.id.clone();
        async move {
            let req = DebateRoundRequest {
                session_id, round: 4, role: role.as_str().to_string(), context, prompt,
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                bot_client::send_debate_request(&client, &endpoint, &token, &req),
            ).await;
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
    }).collect();

    let results = futures::future::join_all(futures).await;

    for (bot_id, resp_opt) in &results {
        let (response_text, confidence, pc_json, abstained) = match resp_opt {
            Some(r) => {
                let pc = r.position_change.as_ref()
                    .and_then(|pc| serde_json::to_string(pc).ok());
                (r.response.clone(), r.confidence, pc, false)
            }
            None => ("(abstained)".to_string(), None, None, true),
        };
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool, &resp_id, debate_id, 4, bot_id, &response_text,
            confidence, None, pc_json.as_deref(), true, 0, abstained,
        ).await.map_err(|e| format!("db error storing Round 4 response: {e}"))?;
    }

    Ok(results)
}
