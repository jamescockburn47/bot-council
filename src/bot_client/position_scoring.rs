//! Pre-Phase-1 legacy dispatch: position (round 0) and scoring requests.
//!
//! These shapes predate the unified `DebateRoundRequest` contract in
//! `mod.rs` and are kept here for callers that still use them. New
//! code should prefer `DebateRoundRequest` and `dispatch_round_request`.

use reqwest_middleware::ClientWithMiddleware;
use serde::{Deserialize, Serialize};

/// Request payload sent to a bot's position endpoint.
#[derive(Debug, Serialize)]
pub struct PositionRequest {
    pub session_id: String,
    pub round: i64,
    pub prompt: String,
}

/// Request payload sent to a bot's scoring endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct ScoringRequest {
    pub session_id: String,
    pub round: String,
    pub context: Vec<ScoringContext>,
    pub prompt: String,
}

/// One entry in the scoring context (a pseudonymised response to evaluate).
#[derive(Debug, Clone, Serialize)]
pub struct ScoringContext {
    pub pseudonym: String,
    pub response: String,
}

/// Response body from a bot's position endpoint.
#[derive(Debug, Deserialize)]
pub struct PositionResponse {
    pub response: String,
}

/// Response body from a bot's scoring endpoint.
#[derive(Debug, Deserialize)]
pub struct ScoringResponse {
    pub scores: Vec<ScoreEntry>,
}

/// A single score entry within a ScoringResponse.
#[derive(Debug, Deserialize)]
pub struct ScoreEntry {
    pub pseudonym: String,
    pub reasoning_quality: i64,
    pub factual_grounding: i64,
    pub overall: i64,
    pub reasoning: String,
}

/// Send a position request to a bot.
pub async fn send_position_request(
    client: &ClientWithMiddleware,
    endpoint_url: &str,
    token: &str,
    request: &PositionRequest,
) -> Result<PositionResponse, String> {
    let mut req = client.post(endpoint_url);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let resp = req
        .json(request)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("bot returned HTTP {}", resp.status()));
    }
    resp.json::<PositionResponse>()
        .await
        .map_err(|e| format!("invalid response body: {e}"))
}

/// Send a scoring request to a bot.
pub async fn send_scoring_request(
    client: &ClientWithMiddleware,
    endpoint_url: &str,
    token: &str,
    request: &ScoringRequest,
) -> Result<ScoringResponse, String> {
    let mut req = client.post(endpoint_url);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let resp = req
        .json(request)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("bot returned HTTP {}", resp.status()));
    }
    resp.json::<ScoringResponse>()
        .await
        .map_err(|e| format!("invalid response body: {e}"))
}
