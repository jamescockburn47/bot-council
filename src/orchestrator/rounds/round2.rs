use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{self, DebateRoundRequest, RoundContext, DebateRoundResponse};
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::analyser::challenge::validate_challenge;
use crate::config::ModelsConfig;
use crate::orchestrator::prompts;
use crate::types::Role;

/// Run Round 2: structured rebuttal with MiniMax challenge validation.
/// Bots that fail validation are re-prompted up to max_retries times.
pub async fn run_round2(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    round1_context: Vec<RoundContext>,
    models_config: &ModelsConfig,
    timeout_secs: u64,
    max_retries: u32,
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    let mut final_results: Vec<(String, Option<DebateRoundResponse>)> = Vec::new();

    // Initial concurrent dispatch
    let initial_futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = debate_id.to_string();
        let role = role_assignments.get(&bot.id).copied().unwrap_or(Role::Proponent);
        let prompt = prompts::round2_prompt();
        let context = round1_context.clone();
        let bot_id = bot.id.clone();
        async move {
            let req = DebateRoundRequest {
                session_id, round: 2, role: role.as_str().to_string(), context, prompt,
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                bot_client::send_debate_request(&client, &endpoint, &token, &req),
            ).await;
            match result {
                Ok(Ok(resp)) => (bot_id, Some(resp)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot_id, error = %e, "Round 2: bot request failed");
                    (bot_id, None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot_id, "Round 2: bot request timed out");
                    (bot_id, None)
                }
            }
        }
    }).collect();

    let initial_results = futures::future::join_all(initial_futures).await;

    // Validate each bot's challenge, re-prompt if needed (sequential per bot)
    for (bot_id, resp_opt) in initial_results {
        match resp_opt {
            None => {
                let resp_id = uuid::Uuid::new_v4().to_string();
                queries_phase1::insert_response_full(
                    pool, &resp_id, debate_id, 2, &bot_id, "(abstained)",
                    None, None, None, true, 0, true,
                ).await.map_err(|e| format!("db error: {e}"))?;
                final_results.push((bot_id, None));
            }
            Some(mut resp) => {
                let bot = bots.iter().find(|b| b.id == bot_id);
                let endpoint = bot.map(|b| b.endpoint_url.as_str()).unwrap_or("").to_string();
                let token = bot_tokens.get(&bot_id).cloned().unwrap_or_default();
                let role = role_assignments.get(&bot_id).copied().unwrap_or(Role::Proponent);
                let mut retry_count: u32 = 0;
                let mut valid = false;

                loop {
                    if let Some(ref challenge) = resp.challenge {
                        let challenge_json = serde_json::to_string(challenge).unwrap_or_default();
                        match validate_challenge(models_config, &challenge_json, &resp.response).await {
                            Ok(v) if v.valid => { valid = true; break; }
                            Ok(v) => {
                                tracing::info!(bot_id = %bot_id, reason = %v.reason, "Round 2: challenge rejected");
                                if retry_count >= max_retries { break; }
                                let reprompt = prompts::round2_reprompt(&v.reason);
                                let req = DebateRoundRequest {
                                    session_id: debate_id.to_string(), round: 2,
                                    role: role.as_str().to_string(),
                                    context: round1_context.clone(), prompt: reprompt,
                                };
                                match bot_client::send_debate_request(client, &endpoint, &token, &req).await {
                                    Ok(new_resp) => { resp = new_resp; retry_count += 1; }
                                    Err(e) => {
                                        tracing::warn!(bot_id = %bot_id, error = %e, "Round 2: re-prompt failed");
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(bot_id = %bot_id, error = %e, "Round 2: MiniMax validation error, accepting");
                                valid = true;
                                break;
                            }
                        }
                    } else {
                        if retry_count >= max_retries {
                            tracing::warn!(bot_id = %bot_id, "Round 2: no challenge after retries");
                            break;
                        }
                        let reprompt = prompts::round2_reprompt("No challenge object found in response");
                        let req = DebateRoundRequest {
                            session_id: debate_id.to_string(), round: 2,
                            role: role.as_str().to_string(),
                            context: round1_context.clone(), prompt: reprompt,
                        };
                        match bot_client::send_debate_request(client, &endpoint, &token, &req).await {
                            Ok(new_resp) => { resp = new_resp; retry_count += 1; }
                            Err(e) => {
                                tracing::warn!(bot_id = %bot_id, error = %e, "Round 2: re-prompt failed");
                                break;
                            }
                        }
                    }
                }

                let challenge_json = resp.challenge.as_ref()
                    .and_then(|c| serde_json::to_string(c).ok());
                let resp_id = uuid::Uuid::new_v4().to_string();
                queries_phase1::insert_response_full(
                    pool, &resp_id, debate_id, 2, &bot_id, &resp.response,
                    resp.confidence, challenge_json.as_deref(), None,
                    valid, retry_count as i64, false,
                ).await.map_err(|e| format!("db error: {e}"))?;

                // Store validation analysis
                if let Some(ref challenge) = resp.challenge {
                    let analysis_id = uuid::Uuid::new_v4().to_string();
                    let input = serde_json::to_string(challenge).unwrap_or_default();
                    let result_val = serde_json::json!({ "valid": valid, "retry_count": retry_count });
                    // intentional: log and continue if analysis insert fails
                    let _ = queries_phase1::insert_analysis(
                        pool, &analysis_id, debate_id, Some(&bot_id),
                        "challenge_validation", &input, &result_val.to_string(),
                        &models_config.minimax_model,
                    ).await;
                }

                final_results.push((bot_id, Some(resp)));
            }
        }
    }

    Ok(final_results)
}
