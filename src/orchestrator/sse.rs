//! SSE emit helpers for the debate pipeline (split from multi_round.rs
//! for the file-length ceiling).

use crate::api::events::DebateEvent;
use crate::types::Role;
use std::collections::HashMap;
use tokio::sync::broadcast;

/// Emit an SSE event. Silently drops if no sender or no listeners.
pub(crate) fn emit(tx: &Option<broadcast::Sender<DebateEvent>>, event: DebateEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(event); // intentional: drop if no listeners
    }
}

/// Helper to emit ResponseReceived + RoundCompleted events after a round finishes.
pub(crate) fn emit_round_responses(
    tx: &Option<broadcast::Sender<DebateEvent>>,
    round_number: i64,
    responses: &[crate::db::models::ResponseRow],
    pseudonym_map: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
) {
    for r in responses {
        let pseudo = pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default();
        let role_str = role_assignments
            .get(&r.bot_id)
            .map(|role| role.to_string())
            .unwrap_or_default();
        emit(
            tx,
            DebateEvent::ResponseReceived {
                round_number,
                pseudonym: pseudo,
                role: role_str,
                response: r.response_json.clone(),
                confidence: r.confidence,
                challenge: r
                    .challenge_json
                    .as_ref()
                    .and_then(|j| serde_json::from_str(j).ok()),
                position_change: r
                    .position_change_json
                    .as_ref()
                    .and_then(|j| serde_json::from_str(j).ok()),
                valid: r.valid,
                abstained: r.abstained,
            },
        );
    }
    let valid_count = responses.iter().filter(|r| r.valid && !r.abstained).count();
    emit(
        tx,
        DebateEvent::RoundCompleted {
            round_number,
            response_count: responses.len(),
            valid_count,
        },
    );
}
