use crate::api::auth::RequireAuth;
use crate::api::dto::*;
use crate::db::{queries, queries_phase1};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, State};
use std::collections::HashMap;

/// GET /debates/{id}/transcript — full round-by-round transcript.
pub async fn get_transcript(
    State(state): State<AppState>,
    _auth: RequireAuth,
    Path(id): Path<String>,
) -> AppResult<Json<TranscriptResponse>> {
    let debate = queries::get_debate(state.db(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("debate {id} not found")))?;

    let debate_bots = queries_phase1::get_debate_bots_with_roles(state.db(), &id).await?;
    let rounds = queries_phase1::get_rounds(state.db(), &id).await?;
    let all_responses = queries_phase1::get_all_responses(state.db(), &id).await?;

    // Build bot_id -> pseudonym map for reuse.
    let bot_pseudonym_map: HashMap<String, String> = debate_bots
        .iter()
        .map(|db| (db.bot_id.clone(), db.pseudonym.clone()))
        .collect();

    // Fetch challenge validation analyses and build bot_id -> reason map.
    let validation_analyses =
        queries_phase1::get_analyses(state.db(), &id, Some("challenge_validation"))
            .await
            .map_err(AppError::Database)?;

    let mut validation_map: HashMap<String, String> = HashMap::new();
    for analysis in &validation_analyses {
        if let Some(bot_id) = &analysis.bot_id {
            if let Ok(result) = serde_json::from_str::<serde_json::Value>(&analysis.result_json) {
                if let Some(reason) = result.get("reason").and_then(|r| r.as_str()) {
                    validation_map.insert(bot_id.clone(), reason.to_string());
                }
            }
        }
    }

    let mut transcript_rounds = Vec::new();
    for round in &rounds {
        let round_responses: Vec<TranscriptEntry> = all_responses
            .iter()
            .filter(|r| r.round_number == round.round_number)
            .map(|r| {
                let pseudonym = bot_pseudonym_map
                    .get(&r.bot_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown".into());
                let challenge = r
                    .challenge_json
                    .as_ref()
                    .and_then(|c| serde_json::from_str(c).ok());
                let position_change = r
                    .position_change_json
                    .as_ref()
                    .and_then(|p| serde_json::from_str(p).ok());
                let validation_reasoning = validation_map.get(&r.bot_id).cloned();
                let extraction_metadata = r
                    .extraction_metadata
                    .as_ref()
                    .and_then(|m| serde_json::from_str(m).ok());
                TranscriptEntry {
                    pseudonym,
                    response: r.response_json.clone(),
                    confidence: r.confidence,
                    challenge,
                    position_change,
                    valid: r.valid,
                    abstained: r.abstained,
                    validation_reasoning,
                    extraction_metadata,
                    fallback_from_round: r.fallback_from_round,
                    retry_count: r.retry_count,
                }
            })
            .collect();

        transcript_rounds.push(TranscriptRound {
            round_number: round.round_number,
            status: round.status.clone(),
            responses: round_responses,
        });
    }

    let anonymisation_log: Vec<AnonymisationEntry> = debate_bots
        .iter()
        .map(|db| AnonymisationEntry {
            pseudonym: db.pseudonym.clone(),
            role: db.role.clone(),
        })
        .collect();

    // Fetch divergence analyses and build DivergenceEntry vec.
    let divergence_rows = queries_phase1::get_analyses(state.db(), &id, Some("divergence"))
        .await
        .map_err(AppError::Database)?;

    let divergence_analyses: Vec<DivergenceEntry> = divergence_rows
        .iter()
        .filter_map(|a| {
            let bot_id = a.bot_id.as_ref()?;
            let pseudonym = bot_pseudonym_map
                .get(bot_id)
                .cloned()
                .unwrap_or_else(|| "Unknown".into());
            let result: serde_json::Value = serde_json::from_str(&a.result_json).ok()?;
            Some(DivergenceEntry {
                pseudonym,
                shifted: result.get("shifted").and_then(|v| v.as_bool()),
                magnitude: result
                    .get("magnitude")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                what_changed: result
                    .get("what_changed")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                justification_adequate: result
                    .get("justification_adequate")
                    .and_then(|v| v.as_bool()),
                flags: result
                    .get("flags")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|f| f.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect();

    // Load the selected crux (if any) from the stored `crux_selection`
    // analysis row. `get_analyses` orders by `created_at`; take the last
    // row so a re-run produces the freshest selection. Older debates have
    // no row and `crux` stays `None`.
    let crux_rows = queries_phase1::get_analyses(state.db(), &id, Some("crux_selection"))
        .await
        .map_err(AppError::Database)?;
    let crux: Option<CruxDto> = crux_rows.last().and_then(|row| {
        serde_json::from_str::<crate::analyser::crux::CruxSelection>(&row.result_json)
            .ok()
            .map(|sel| CruxDto {
                claim: sel.claim,
                source_pseudonym: sel.source_pseudonym,
                source_quote: sel.source_quote,
            })
    });

    Ok(Json(TranscriptResponse {
        debate_id: id,
        topic: debate.topic,
        rounds: transcript_rounds,
        anonymisation_log,
        divergence_analyses,
        crux,
    }))
}
