//! SSE endpoint for streaming debate lifecycle events.

use std::convert::Infallible;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, State},
    http::header,
    response::IntoResponse,
};
use tokio_stream::StreamExt;

use crate::api::auth::AuthIdentity;
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// GET /debates/{id}/stream -- SSE stream of debate lifecycle events.
///
/// Returns `text/event-stream` with debate events as they occur.
/// Sends a keepalive comment every 30 seconds to prevent proxy timeouts.
///
/// Returns 404 if the debate does not exist or has no active stream.
/// Returns 409 if the debate is in a terminal state.
pub async fn stream_debate(
    State(state): State<AppState>,
    _auth: AuthIdentity,
    Path(debate_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    // Verify debate exists
    let debate = queries::get_debate(state.db(), &debate_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("debate {debate_id} not found")))?;

    // Reject if already terminal
    let terminal = ["complete", "cancelled", "failed"];
    if terminal.contains(&debate.status.as_str()) {
        return Err(AppError::Conflict(
            "Debate is already complete. Use the REST API to fetch results.".into(),
        ));
    }

    // Subscribe to the broadcast channel
    let rx = state
        .subscribe_debate_stream(&debate_id)
        .ok_or_else(|| AppError::NotFound("no active stream for this debate".into()))?;

    // Convert broadcast receiver into an SSE stream
    let event_stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(
        |result| match result {
            Ok(event) => {
                let json = serde_json::to_string(&event).unwrap_or_default();
                let sse = format!("event: {}\ndata: {}\n\n", event.event_type(), json);
                Some(Ok::<_, Infallible>(sse))
            }
            Err(_) => None, // intentional: skip lagged messages
        },
    );

    // Keepalive every 30s to prevent proxy timeouts
    let keepalive = tokio_stream::wrappers::IntervalStream::new(
        tokio::time::interval(Duration::from_secs(30)),
    )
    .map(|_| Ok::<_, Infallible>(":keepalive\n\n".to_string()));

    // Merge event stream with keepalive
    let merged = tokio_stream::StreamExt::merge(event_stream, keepalive);

    Ok((
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(merged),
    ))
}
