use axum::extract::{Path, State};
use axum::Json;
use crate::api::auth::BearerAuth;
use crate::api::dto::*;
use crate::db::{queries, queries_phase1};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// GET /debates/{id}/transcript — full round-by-round transcript.
pub async fn get_transcript(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Path(id): Path<String>,
) -> AppResult<Json<TranscriptResponse>> {
    let debate = queries::get_debate(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound(format!("debate {id} not found")))?;

    let debate_bots = queries_phase1::get_debate_bots_with_roles(state.db(), &id).await?;
    let rounds = queries_phase1::get_rounds(state.db(), &id).await?;
    let all_responses = queries_phase1::get_all_responses(state.db(), &id).await?;

    let mut transcript_rounds = Vec::new();
    for round in &rounds {
        let round_responses: Vec<TranscriptEntry> = all_responses.iter()
            .filter(|r| r.round_number == round.round_number)
            .map(|r| {
                let pseudonym = debate_bots.iter()
                    .find(|db| db.bot_id == r.bot_id)
                    .map(|db| db.pseudonym.clone())
                    .unwrap_or_else(|| "Unknown".into());
                let challenge = r.challenge_json.as_ref()
                    .and_then(|c| serde_json::from_str(c).ok());
                let position_change = r.position_change_json.as_ref()
                    .and_then(|p| serde_json::from_str(p).ok());
                TranscriptEntry {
                    pseudonym,
                    response: r.response_json.clone(),
                    confidence: r.confidence,
                    challenge,
                    position_change,
                    valid: r.valid,
                    abstained: r.abstained,
                }
            })
            .collect();

        transcript_rounds.push(TranscriptRound {
            round_number: round.round_number,
            status: round.status.clone(),
            responses: round_responses,
        });
    }

    let anonymisation_log: Vec<AnonymisationEntry> = debate_bots.iter()
        .map(|db| AnonymisationEntry {
            pseudonym: db.pseudonym.clone(),
            role: db.role.clone(),
        })
        .collect();

    Ok(Json(TranscriptResponse {
        debate_id: id,
        topic: debate.topic,
        rounds: transcript_rounds,
        anonymisation_log,
    }))
}
