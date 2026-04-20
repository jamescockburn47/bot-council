use crate::analyser::pairing::compute_pairings;
use crate::bot_client::{self, DebateRoundRequest, DebateRoundResponse};
use crate::config::ModelsConfig;
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::orchestrator::prompts;
use crate::types::Role;
use reqwest_middleware::ClientWithMiddleware;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Run Round 3: cross-examination in two passes.
/// Pass A: each bot poses a question to its partner (concurrent).
/// Pass B: each bot answers the question posed to it (concurrent).
pub async fn run_round3(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    _pseudonym_map: &HashMap<String, String>,
    reverse_pseudonym_map: &HashMap<String, String>,
    round2_responses: &HashMap<String, String>,
    models_config: &ModelsConfig,
    timeout_secs: u64,
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    let topic = topic.to_string();
    // Step 1: Compute pairings via MiniMax
    let positions: Vec<(String, String)> = round2_responses
        .iter()
        .map(|(p, r)| (p.clone(), r.clone()))
        .collect();
    let pairing = compute_pairings(models_config, &positions).await?;

    // Resolve pseudonym -> bot_id
    let resolve = |pseudo: &str| -> String {
        reverse_pseudonym_map
            .get(pseudo)
            .cloned()
            .unwrap_or_default()
    };

    // Store pairing
    let pair1_a = resolve(&pairing.pair_1[0]);
    let pair1_b = resolve(&pairing.pair_1[1]);
    let pairing_json = serde_json::to_string(&pairing).unwrap_or_default();
    let third_bot = resolve(&pairing.third);
    queries_phase1::insert_pairing(
        pool,
        debate_id,
        &pair1_a,
        &pair1_b,
        Some(&third_bot),
        &pairing_json,
    )
    .await
    .map_err(|e| format!("db error storing pairing: {e}"))?;

    // Build directed question targets: (questioner_pseudo, target_pseudo)
    let mut question_targets: Vec<(String, String)> = Vec::new();
    question_targets.push((pairing.pair_1[0].clone(), pairing.pair_1[1].clone()));
    question_targets.push((pairing.pair_1[1].clone(), pairing.pair_1[0].clone()));
    question_targets.push((pairing.pair_2[0].clone(), pairing.pair_2[1].clone()));
    question_targets.push((pairing.pair_2[1].clone(), pairing.pair_2[0].clone()));
    // Third joins one pair — round-robin addition
    let joined_pair = if pairing.third_joins == "pair_1" {
        &pairing.pair_1
    } else {
        &pairing.pair_2
    };
    question_targets.push((pairing.third.clone(), joined_pair[0].clone()));
    question_targets.push((joined_pair[0].clone(), pairing.third.clone()));

    // Pass A: Each bot poses a question (concurrent)
    let pass_a_futures: Vec<_> = question_targets
        .iter()
        .map(|(q_pseudo, t_pseudo)| {
            let q_bot_id = resolve(q_pseudo);
            let bot = bots.iter().find(|b| b.id == q_bot_id);
            let endpoint = bot.map(|b| b.endpoint_url.clone()).unwrap_or_default();
            let token = bot_tokens.get(&q_bot_id).cloned().unwrap_or_default();
            let role = role_assignments
                .get(&q_bot_id)
                .copied()
                .unwrap_or(Role::Proponent);
            let target_response = round2_responses
                .get(t_pseudo.as_str())
                .cloned()
                .unwrap_or_default();
            let prompt = prompts::round3_question_prompt(&topic, t_pseudo, &target_response);
            let session_id = debate_id.to_string();
            let client = client.clone();
            let bot_id = q_bot_id.clone();
            async move {
                let req = DebateRoundRequest {
                    session_id,
                    round: 3,
                    role: role.as_str().to_string(),
                    context: vec![],
                    prompt,
                };
                let result = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout_secs),
                    bot_client::send_debate_request(&client, &endpoint, &token, &req),
                )
                .await;
                match result {
                    Ok(Ok(resp)) => (bot_id, Some(resp.response)),
                    Ok(Err(e)) => {
                        tracing::warn!(bot_id = %bot_id, error = %e, "Round 3 Pass A: failed");
                        (bot_id, None)
                    }
                    Err(_) => {
                        tracing::warn!(bot_id = %bot_id, "Round 3 Pass A: timed out");
                        (bot_id, None)
                    }
                }
            }
        })
        .collect();

    let pass_a_results = futures::future::join_all(pass_a_futures).await;

    // Build question map: target_bot_id -> Vec<(questioner_pseudo, question_text)>
    let mut questions_for: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for ((q_pseudo, t_pseudo), (_, question_opt)) in
        question_targets.iter().zip(pass_a_results.iter())
    {
        if let Some(question) = question_opt {
            let target_bot_id = resolve(t_pseudo);
            questions_for
                .entry(target_bot_id)
                .or_default()
                .push((q_pseudo.clone(), question.clone()));
        }
    }

    // Pass B: Each bot answers ALL questions posed to it (concurrent).
    // With 5 bots, some bots receive 2 questions (the joined pair member).
    // We combine all questions into one prompt so the bot addresses each.
    let pass_b_futures: Vec<_> = bots.iter().filter_map(|bot| {
        let questions = questions_for.get(&bot.id)?;
        if questions.is_empty() { return None; }

        // Build a combined prompt with all questions posed to this bot
        let combined_prompt = if questions.len() == 1 {
            let (q_pseudo, q_text) = &questions[0];
            let partner_response = round2_responses.get(q_pseudo.as_str()).cloned().unwrap_or_default();
            prompts::round3_answer_prompt(&topic, q_pseudo, &partner_response, q_text)
        } else {
            // Multiple questioners — format all questions
            let mut parts = Vec::new();
            for (q_pseudo, q_text) in questions {
                let partner_response = round2_responses.get(q_pseudo.as_str()).cloned().unwrap_or_default();
                parts.push(format!(
                    "Question from {q_pseudo} (whose position was: {partner_response}):\n\"{q_text}\""
                ));
            }
            format!(
                "Topic: {topic}\n\
                 You are being cross-examined by multiple participants.\n\n{}\n\n\
                 Address each question directly and substantively.",
                parts.join("\n\n")
            )
        };

        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let role = role_assignments.get(&bot.id).copied().unwrap_or(Role::Proponent);
        let session_id = debate_id.to_string();
        let bot_id = bot.id.clone();
        Some(async move {
            let req = DebateRoundRequest {
                session_id, round: 3, role: role.as_str().to_string(),
                context: vec![], prompt: combined_prompt,
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                bot_client::send_debate_request(&client, &endpoint, &token, &req),
            ).await;
            match result {
                Ok(Ok(resp)) => (bot_id, Some(resp)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot_id, error = %e, "Round 3 Pass B: failed");
                    (bot_id, None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot_id, "Round 3 Pass B: timed out");
                    (bot_id, None)
                }
            }
        })
    }).collect();

    let pass_b_results = futures::future::join_all(pass_b_futures).await;

    // Store combined responses: question(s) posed TO this bot + their answer
    let mut all_results: Vec<(String, Option<DebateRoundResponse>)> = Vec::new();
    for (bot_id, answer_opt) in &pass_b_results {
        // Get question(s) that were posed TO this bot
        let questions = questions_for.get(bot_id.as_str());
        let question_summary = questions.map(|qs| {
            qs.iter()
                .map(|(pseudo, text)| format!("[Question from {pseudo}]: {text}"))
                .collect::<Vec<_>>()
                .join("\n\n")
        });
        let combined = match (&question_summary, answer_opt) {
            (Some(q), Some(a)) => format!("{q}\n\n[Answer given]: {}", a.response),
            (Some(q), None) => format!("{q}\n\n[Answer]: (no answer)"),
            (None, Some(a)) => format!("[Question]: (none)\n\n[Answer given]: {}", a.response),
            (None, None) => "(abstained)".to_string(),
        };
        let abstained = answer_opt.is_none() && question_summary.is_none();
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool,
            &resp_id,
            debate_id,
            3,
            bot_id,
            &combined,
            answer_opt.as_ref().and_then(|r| r.confidence),
            None,
            None,
            true,
            0,
            abstained,
        )
        .await
        .map_err(|e| format!("db error storing Round 3 response: {e}"))?;
        all_results.push((bot_id.clone(), answer_opt.clone()));
    }

    Ok(all_results)
}
