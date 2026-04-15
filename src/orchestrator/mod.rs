pub mod anonymiser;
pub mod roles;
pub mod prompts;
pub mod state_machine;

use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{
    self, PositionRequest, ScoringRequest, ScoringContext,
};
use crate::db::{models::BotRow, queries};
use crate::types::DebateId;

/// Result of a completed debate.
pub struct DebateResult {
    pub debate_id: String,
    pub rankings: Vec<RankedEntry>,
}

/// A ranked argument with aggregated scores.
pub struct RankedEntry {
    pub pseudonym: String,
    pub avg_reasoning_quality: f64,
    pub avg_factual_grounding: f64,
    pub avg_overall: f64,
    pub total_scores: usize,
}

/// Run a single-shot debate: dispatch topic, collect responses, score, aggregate.
pub async fn run_debate(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &DebateId,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
) -> Result<DebateResult, String> {
    let id = debate_id.as_str();

    queries::update_debate_status(pool, id, "dispatching")
        .await.map_err(|e| format!("db error: {e}"))?;

    // Step 1: Dispatch topic to all bots concurrently
    let position_futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = id.to_string();
        let topic = topic.to_string();
        async move {
            let req = PositionRequest {
                session_id,
                round: 0,
                prompt: format!(
                    "You are participating in a structured debate.\nTopic: {}\n\nState your position. Be substantive and specific.",
                    topic
                ),
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(300),
                bot_client::send_position_request(&client, &endpoint, &token, &req),
            ).await;
            match result {
                Ok(Ok(resp)) => (bot.id.clone(), Some(resp.response)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot.id, error = %e, "bot position request failed");
                    (bot.id.clone(), None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot.id, "bot position request timed out");
                    (bot.id.clone(), None)
                }
            }
        }
    }).collect();

    let position_results = futures::future::join_all(position_futures).await;

    // Store responses and build anonymised context
    let debate_bots = queries::get_debate_bots(pool, id)
        .await.map_err(|e| format!("db error: {e}"))?;

    let mut anonymised: Vec<ScoringContext> = Vec::new();
    for (bot_id, response_opt) in &position_results {
        let pseudonym = debate_bots.iter()
            .find(|db| db.bot_id == *bot_id)
            .map(|db| db.pseudonym.clone())
            .unwrap_or_else(|| "Unknown".to_string());
        let (response_json, abstained) = match response_opt {
            Some(text) => (text.clone(), false),
            None => ("(abstained)".to_string(), true),
        };
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries::insert_response(pool, &resp_id, id, 0, bot_id, &response_json, abstained)
            .await.map_err(|e| format!("db error: {e}"))?;
        if !abstained {
            anonymised.push(ScoringContext { pseudonym, response: response_json });
        }
    }

    // Check quorum
    if anonymised.len() < 3 {
        queries::update_debate_status(pool, id, "failed")
            .await.map_err(|e| format!("db error: {e}"))?;
        return Err(format!("quorum not met: only {} bots responded", anonymised.len()));
    }

    // Step 2: Send scoring requests concurrently
    queries::update_debate_status(pool, id, "scoring")
        .await.map_err(|e| format!("db error: {e}"))?;

    let scoring_futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = id.to_string();
        let own_pseudonym = debate_bots.iter()
            .find(|db| db.bot_id == bot.id)
            .map(|db| db.pseudonym.clone())
            .unwrap_or_default();
        let context: Vec<ScoringContext> = anonymised.iter()
            .filter(|c| c.pseudonym != own_pseudonym)
            .cloned()
            .collect();
        async move {
            let req = ScoringRequest {
                session_id,
                round: "scoring".to_string(),
                context,
                prompt: "Score each argument 0-10 on reasoning_quality and factual_grounding. Return JSON with a scores array.".to_string(),
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(300),
                bot_client::send_scoring_request(&client, &endpoint, &token, &req),
            ).await;
            match result {
                Ok(Ok(resp)) => (bot.id.clone(), Some(resp.scores)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot.id, error = %e, "bot scoring request failed");
                    (bot.id.clone(), None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot.id, "bot scoring request timed out");
                    (bot.id.clone(), None)
                }
            }
        }
    }).collect();

    let scoring_results = futures::future::join_all(scoring_futures).await;

    // Store scores
    for (scorer_bot_id, scores_opt) in &scoring_results {
        if let Some(scores) = scores_opt {
            for score in scores {
                let score_id = uuid::Uuid::new_v4().to_string();
                if let Err(e) = queries::insert_peer_score(
                    pool, &score_id, id, scorer_bot_id, &score.pseudonym,
                    score.reasoning_quality, score.factual_grounding,
                    score.overall, &score.reasoning,
                ).await {
                    // intentional: log and continue — one bad score shouldn't fail the debate
                    tracing::warn!(scorer = %scorer_bot_id, target = %score.pseudonym, error = %e, "failed to store peer score");
                }
            }
        }
    }

    // Step 3: Aggregate scores into rankings
    let all_scores = queries::get_peer_scores(pool, id)
        .await.map_err(|e| format!("db error: {e}"))?;

    let pseudonyms: Vec<String> = anonymised.iter().map(|c| c.pseudonym.clone()).collect();
    let mut rankings: Vec<RankedEntry> = pseudonyms.iter().map(|p| {
        let scores: Vec<_> = all_scores.iter().filter(|s| s.target_pseudonym == *p).collect();
        let count = scores.len();
        if count == 0 {
            return RankedEntry {
                pseudonym: p.clone(),
                avg_reasoning_quality: 0.0, avg_factual_grounding: 0.0,
                avg_overall: 0.0, total_scores: 0,
            };
        }
        RankedEntry {
            pseudonym: p.clone(),
            avg_reasoning_quality: scores.iter().map(|s| s.reasoning_quality as f64).sum::<f64>() / count as f64,
            avg_factual_grounding: scores.iter().map(|s| s.factual_grounding as f64).sum::<f64>() / count as f64,
            avg_overall: scores.iter().map(|s| s.overall as f64).sum::<f64>() / count as f64,
            total_scores: count,
        }
    }).collect();

    rankings.sort_by(|a, b| b.avg_overall.partial_cmp(&a.avg_overall).unwrap_or(std::cmp::Ordering::Equal));

    queries::update_debate_status(pool, id, "complete")
        .await.map_err(|e| format!("db error: {e}"))?;

    Ok(DebateResult { debate_id: id.to_string(), rankings })
}
