use crate::analyser::challenge::validate_challenge;
use crate::bot_client::{self, DebateRoundRequest, DebateRoundResponse, RoundContext};
use crate::config::ModelsConfig;
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::orchestrator::{prompts, response_parser};
use crate::types::Role;
use reqwest_middleware::ClientWithMiddleware;
use sqlx::SqlitePool;
use std::collections::HashMap;

async fn send_round2_request_with_timeout(
    client: &ClientWithMiddleware,
    bot_kind: &str,
    endpoint: &str,
    token: &str,
    req: &DebateRoundRequest,
    timeout_secs: u64,
) -> Result<DebateRoundResponse, String> {
    match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        bot_client::dispatch_round_request(client, bot_kind, endpoint, token, req),
    )
    .await
    {
        Ok(Ok(resp)) => Ok(resp),
        Ok(Err(err)) => Err(format!("request failed: {err}")),
        Err(_) => Err(format!("request timed out after {timeout_secs}s")),
    }
}

/// Run Round 2 in either:
/// - strict mode: structured rebuttal with MiniMax challenge validation
/// - simple mode: final-position round with response-only contract
pub async fn run_round2(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    round1_context: Vec<RoundContext>,
    models_config: &ModelsConfig,
    timeout_secs: u64,
    max_retries: u32,
    strict_validation: bool,
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    let mut final_results: Vec<(String, Option<DebateRoundResponse>)> = Vec::new();
    let topic = topic.to_string();

    // Initial concurrent dispatch
    let initial_futures: Vec<_> = bots
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
            let prompt = if strict_validation {
                prompts::round2_prompt(&topic)
            } else {
                prompts::round2_prompt_simple(&topic, role)
            };
            let context = round1_context.clone();
            let bot_id = bot.id.clone();
            async move {
                let req = DebateRoundRequest {
                    session_id,
                    round: 2,
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
                        tracing::warn!(bot_id = %bot_id, error = %e, "Round 2: bot request failed");
                        (bot_id, None)
                    }
                    Err(_) => {
                        tracing::warn!(bot_id = %bot_id, "Round 2: bot request timed out");
                        (bot_id, None)
                    }
                }
            }
        })
        .collect();

    let initial_results = futures::future::join_all(initial_futures).await;

    // Validate each bot's challenge, re-prompt if needed (sequential per bot)
    for (bot_id, resp_opt) in initial_results {
        match resp_opt {
            None => {
                let resp_id = uuid::Uuid::new_v4().to_string();
                queries_phase1::insert_response_full(
                    pool,
                    &resp_id,
                    debate_id,
                    2,
                    &bot_id,
                    "(abstained)",
                    None,
                    None,
                    None,
                    true,
                    0,
                    true,
                    None,
                    None,
                )
                .await
                .map_err(|e| format!("db error: {e}"))?;
                final_results.push((bot_id, None));
            }
            Some(mut resp) => {
                response_parser::normalise_response(&mut resp);
                if !strict_validation {
                    let challenge_json = resp
                        .challenge
                        .as_ref()
                        .and_then(|c| serde_json::to_string(c).ok());
                    let resp_id = uuid::Uuid::new_v4().to_string();
                    queries_phase1::insert_response_full(
                        pool,
                        &resp_id,
                        debate_id,
                        2,
                        &bot_id,
                        &resp.response,
                        resp.confidence,
                        challenge_json.as_deref(),
                        None,
                        true,
                        0,
                        false,
                        None,
                        None,
                    )
                    .await
                    .map_err(|e| format!("db error: {e}"))?;
                    final_results.push((bot_id, Some(resp)));
                    continue;
                }

                let bot = bots.iter().find(|b| b.id == bot_id);
                let endpoint = bot
                    .map(|b| b.endpoint_url.as_str())
                    .unwrap_or("")
                    .to_string();
                let bot_kind = bot
                    .map(|b| b.bot_kind.as_str())
                    .unwrap_or("external")
                    .to_string();
                let token = bot_tokens.get(&bot_id).cloned().unwrap_or_default();
                let role = role_assignments
                    .get(&bot_id)
                    .copied()
                    .unwrap_or(Role::Proponent);
                let mut retry_count: u32 = 0;
                let mut valid = false;

                loop {
                    if let Some(ref challenge) = resp.challenge {
                        let challenge_json = serde_json::to_string(challenge).unwrap_or_default();
                        match validate_challenge(models_config, &challenge_json, &resp.response)
                            .await
                        {
                            Ok(v) if v.valid => {
                                valid = true;
                                break;
                            }
                            Ok(v) => {
                                tracing::info!(bot_id = %bot_id, reason = %v.reason, "Round 2: challenge rejected");
                                if retry_count >= max_retries {
                                    break;
                                }
                                let reprompt = prompts::round2_reprompt(&topic, &v.reason);
                                let req = DebateRoundRequest {
                                    session_id: debate_id.to_string(),
                                    round: 2,
                                    role: role.as_str().to_string(),
                                    context: round1_context.clone(),
                                    prompt: reprompt,
                                };
                                match send_round2_request_with_timeout(
                                    client,
                                    &bot_kind,
                                    &endpoint,
                                    &token,
                                    &req,
                                    timeout_secs,
                                )
                                .await
                                {
                                    Ok(new_resp) => {
                                        resp = new_resp;
                                        retry_count += 1;
                                    }
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
                        let reprompt = prompts::round2_reprompt(
                            &topic,
                            "No challenge object found in response",
                        );
                        let req = DebateRoundRequest {
                            session_id: debate_id.to_string(),
                            round: 2,
                            role: role.as_str().to_string(),
                            context: round1_context.clone(),
                            prompt: reprompt,
                        };
                        match send_round2_request_with_timeout(
                            client,
                            &bot_kind,
                            &endpoint,
                            &token,
                            &req,
                            timeout_secs,
                        )
                        .await
                        {
                            Ok(new_resp) => {
                                resp = new_resp;
                                retry_count += 1;
                            }
                            Err(e) => {
                                tracing::warn!(bot_id = %bot_id, error = %e, "Round 2: re-prompt failed");
                                break;
                            }
                        }
                    }
                }

                // Post-round extraction for text_only bots whose response
                // lacks a structured challenge field. No-op for external bots
                // (short-circuits on bot_kind check). Runs AFTER the retry
                // loop's final accepted response, BEFORE persist.
                let provenance = crate::orchestrator::extraction::extract_if_needed(
                    models_config,
                    &bot_kind,
                    crate::extractor::ExtractTarget::Challenge,
                    &mut resp,
                )
                .await;
                let extraction_metadata_json = serde_json::to_string(&serde_json::json!({
                    "challenge": provenance.to_json()
                }))
                .ok();

                let challenge_json = resp
                    .challenge
                    .as_ref()
                    .and_then(|c| serde_json::to_string(c).ok());
                let resp_id = uuid::Uuid::new_v4().to_string();
                queries_phase1::insert_response_full(
                    pool,
                    &resp_id,
                    debate_id,
                    2,
                    &bot_id,
                    &resp.response,
                    resp.confidence,
                    challenge_json.as_deref(),
                    None,
                    valid,
                    retry_count as i64,
                    false,
                    extraction_metadata_json.as_deref(),
                    None,
                )
                .await
                .map_err(|e| format!("db error: {e}"))?;

                // Store validation analysis
                if let Some(ref challenge) = resp.challenge {
                    let analysis_id = uuid::Uuid::new_v4().to_string();
                    let input = serde_json::to_string(challenge).unwrap_or_default();
                    let result_val =
                        serde_json::json!({ "valid": valid, "retry_count": retry_count });
                    // intentional: log and continue if analysis insert fails
                    let _ = queries_phase1::insert_analysis(
                        pool,
                        &analysis_id,
                        debate_id,
                        Some(&bot_id),
                        "challenge_validation",
                        &input,
                        &result_val.to_string(),
                        models_config.effective_analysis_model(),
                    )
                    .await;
                }

                final_results.push((bot_id, Some(resp)));
            }
        }
    }

    Ok(final_results)
}

#[cfg(test)]
mod tests {
    use super::send_round2_request_with_timeout;
    use crate::bot_client::{DebateRoundRequest, build_http_client};
    use crate::config::HttpClientConfig;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn round2_request_timeout_wrapper_stops_slow_reprompt() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/debate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(std::time::Duration::from_millis(1500))
                    .set_body_string(r#"{"response":"late"}"#),
            )
            .mount(&server)
            .await;

        let client = build_http_client(&HttpClientConfig {
            connect_timeout_secs: 2,
            request_timeout_secs: 30,
            max_retries: 0,
            retry_delay_secs: 1,
        });
        let req = DebateRoundRequest {
            session_id: "debate".into(),
            round: 2,
            role: "proponent".into(),
            context: vec![],
            prompt: "prompt".into(),
        };

        let start = std::time::Instant::now();
        let result = send_round2_request_with_timeout(
            &client,
            "external",
            &format!("{}/debate", server.uri()),
            "",
            &req,
            1,
        )
        .await;
        let elapsed = start.elapsed();

        assert!(result.is_err(), "expected timeout error");
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "timeout wrapper should not wait for full HTTP timeout; elapsed={elapsed:?}"
        );
    }
}
