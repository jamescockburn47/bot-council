// src/api/debates.rs
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crate::api::auth::BearerAuth;
use crate::api::dto::*;
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::orchestrator;
use crate::orchestrator::anonymiser;
use crate::state::AppState;
use crate::types::DebateId;

/// POST /debates — create and run a debate.
pub async fn create_debate(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Json(req): Json<CreateDebateRequest>,
) -> AppResult<(StatusCode, Json<DebateResponse>)> {
    if req.topic.is_empty() {
        return Err(AppError::BadRequest("topic is required".into()));
    }

    let bots = match &req.bot_ids {
        Some(ids) if !ids.is_empty() => queries::get_bots_by_ids(state.db(), ids).await?,
        _ => queries::list_active_bots(state.db()).await?,
    };

    if bots.len() < 3 {
        return Err(AppError::BadRequest(format!("need at least 3 bots, found {}", bots.len())));
    }

    let debate_id = DebateId::new();
    queries::insert_debate(state.db(), debate_id.as_str(), &req.topic).await?;

    let mut bot_tokens = std::collections::HashMap::new();
    for (i, bot) in bots.iter().enumerate() {
        let pseudonym = anonymiser::assign_pseudonym(i);
        queries::insert_debate_bot(state.db(), debate_id.as_str(), &bot.id, &pseudonym).await?;
        bot_tokens.insert(bot.id.clone(), String::new());
    }

    // Spawn debate as background task
    let pool = state.db().clone();
    let client = state.http_client().clone();
    let topic = req.topic.clone();
    let debate_id_clone = debate_id.clone();
    let bots_clone = bots.clone();
    tokio::spawn(async move {
        match orchestrator::run_debate(&pool, &client, &debate_id_clone, &topic, &bots_clone, &bot_tokens).await {
            Ok(result) => tracing::info!(debate_id = %result.debate_id, rankings = result.rankings.len(), "debate completed"),
            Err(e) => tracing::error!(debate_id = %debate_id_clone, error = %e, "debate failed"),
        }
    });

    let debate_bots = queries::get_debate_bots(state.db(), debate_id.as_str()).await?;
    let bot_infos: Vec<DebateBotInfo> = debate_bots.iter().map(|db| {
        let bot = bots.iter().find(|b| b.id == db.bot_id);
        DebateBotInfo {
            bot_id: db.bot_id.clone(),
            bot_name: bot.map(|b| b.name.clone()).unwrap_or_default(),
            pseudonym: db.pseudonym.clone(),
        }
    }).collect();

    Ok((StatusCode::CREATED, Json(DebateResponse {
        id: debate_id.to_string(),
        topic: req.topic,
        status: "created".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        completed_at: None,
        bots: bot_infos,
        results: None,
    })))
}

/// GET /debates — list debates.
pub async fn list_debates(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Query(params): Query<ListDebatesQuery>,
) -> AppResult<Json<Vec<DebateResponse>>> {
    let limit = params.limit.unwrap_or(20);
    let rows = queries::list_debates(state.db(), params.status.as_deref(), limit).await?;
    let all_bots = queries::list_active_bots(state.db()).await?;

    let mut debates = Vec::new();
    for row in rows {
        let debate_bots = queries::get_debate_bots(state.db(), &row.id).await?;
        let bot_infos: Vec<DebateBotInfo> = debate_bots.iter().map(|db| {
            let bot = all_bots.iter().find(|b| b.id == db.bot_id);
            DebateBotInfo {
                bot_id: db.bot_id.clone(),
                bot_name: bot.map(|b| b.name.clone()).unwrap_or_default(),
                pseudonym: db.pseudonym.clone(),
            }
        }).collect();
        debates.push(DebateResponse {
            id: row.id, topic: row.topic, status: row.status,
            created_at: row.created_at, completed_at: row.completed_at,
            bots: bot_infos, results: None,
        });
    }
    Ok(Json(debates))
}

/// GET /debates/{id} — get debate detail with results if complete.
pub async fn get_debate(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Path(id): Path<String>,
) -> AppResult<Json<DebateResponse>> {
    let debate = queries::get_debate(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound(format!("debate {id} not found")))?;

    let debate_bots = queries::get_debate_bots(state.db(), &id).await?;
    let all_bots = queries::list_active_bots(state.db()).await?;
    let bot_infos: Vec<DebateBotInfo> = debate_bots.iter().map(|db| {
        let bot = all_bots.iter().find(|b| b.id == db.bot_id);
        DebateBotInfo {
            bot_id: db.bot_id.clone(),
            bot_name: bot.map(|b| b.name.clone()).unwrap_or_default(),
            pseudonym: db.pseudonym.clone(),
        }
    }).collect();

    let results = if debate.status == "complete" {
        let responses = queries::get_responses(state.db(), &id, 0).await?;
        let scores = queries::get_peer_scores(state.db(), &id).await?;

        let anon_responses: Vec<AnonymisedResponse> = responses.iter().map(|r| {
            let pseudonym = debate_bots.iter()
                .find(|db| db.bot_id == r.bot_id)
                .map(|db| db.pseudonym.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            AnonymisedResponse { pseudonym, response: r.response_json.clone(), abstained: r.abstained }
        }).collect();

        let pseudonyms: Vec<String> = debate_bots.iter().map(|db| db.pseudonym.clone()).collect();
        let mut rankings: Vec<RankedArgument> = pseudonyms.iter().map(|p| {
            let s: Vec<_> = scores.iter().filter(|s| s.target_pseudonym == *p).collect();
            let count = s.len();
            if count == 0 {
                return RankedArgument {
                    pseudonym: p.clone(), avg_reasoning_quality: 0.0,
                    avg_factual_grounding: 0.0, avg_overall: 0.0, total_scores: 0,
                };
            }
            RankedArgument {
                pseudonym: p.clone(),
                avg_reasoning_quality: s.iter().map(|x| x.reasoning_quality as f64).sum::<f64>() / count as f64,
                avg_factual_grounding: s.iter().map(|x| x.factual_grounding as f64).sum::<f64>() / count as f64,
                avg_overall: s.iter().map(|x| x.overall as f64).sum::<f64>() / count as f64,
                total_scores: count,
            }
        }).collect();
        rankings.sort_by(|a, b| b.avg_overall.partial_cmp(&a.avg_overall).unwrap_or(std::cmp::Ordering::Equal));

        Some(DebateResults { responses: anon_responses, rankings })
    } else {
        None
    };

    Ok(Json(DebateResponse {
        id: debate.id, topic: debate.topic, status: debate.status,
        created_at: debate.created_at, completed_at: debate.completed_at,
        bots: bot_infos, results,
    }))
}
