use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::config::HttpClientConfig;
use crate::sanitise::MAX_RESPONSE_BYTES;

/// Build the HTTP client with retry middleware.
pub fn build_http_client(config: &HttpClientConfig) -> ClientWithMiddleware {
    let retry_policy = ExponentialBackoff::builder()
        .retry_bounds(
            Duration::from_secs(config.retry_delay_secs),
            Duration::from_secs(config.retry_delay_secs * 4),
        )
        .build_with_max_retries(config.max_retries);
    let base = Client::builder()
        .timeout(Duration::from_secs(config.request_timeout_secs))
        .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
        .build()
        .expect("failed to build reqwest client");
    ClientBuilder::new(base)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
}

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
    let resp = client
        .post(endpoint_url)
        .bearer_auth(token)
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
    let resp = client
        .post(endpoint_url)
        .bearer_auth(token)
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

/// Context entry sent to bots in Rounds 1+. Contains anonymised prior responses.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RoundContext {
    /// Stable pseudonym identifying the contributing bot within this debate.
    pub pseudonym: String,
    /// Zero-based round index this response was originally produced in.
    pub round: i64,
    /// The argument text as produced by the bot.
    pub response: String,
    /// Confidence score (0-100); absent for round 0 and abstentions.
    pub confidence: Option<i64>,
}

/// Phase 1 request payload for all rounds. Superset of Phase 0 PositionRequest.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DebateRoundRequest {
    /// Stable debate session identifier (not the bot's internal session).
    pub session_id: String,
    /// Zero-based round index (0..=4).
    pub round: i64,
    /// Constitutional role assigned for this round (e.g. "proponent", "skeptic").
    pub role: String,
    /// Prior anonymised responses this bot should consider; empty in round 0.
    pub context: Vec<RoundContext>,
    /// Round-specific question/instruction prepared by the orchestrator.
    pub prompt: String,
}

/// Structured challenge object (Round 2 required, optional other rounds).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChallengeField {
    /// Quoted claim being challenged (verbatim from the target's response).
    pub claim_targeted: String,
    /// Evidence or reasoning undermining the claim.
    pub counter_evidence: String,
    /// Challenge type label (e.g. "factual", "logical", "scope").
    #[serde(rename = "type")]
    #[schemars(rename = "type")]
    pub challenge_type: String,
}

/// Position change declaration (Round 4 required).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PositionChangeField {
    /// True if the bot materially changed position during the debate.
    pub changed: bool,
    /// One-sentence summary of the position the bot held at round 0.
    pub from_summary: String,
    /// One-sentence summary of the position held after round 4.
    pub to_summary: String,
    /// Rationale for the change (or why it didn't change).
    pub reason: String,
}

/// Phase 1 response from a bot. All fields after `response` are optional
/// depending on the round.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DebateRoundResponse {
    /// The bot's argument text for this round.
    pub response: String,
    /// Confidence in the argument, 0–100 (peer-scoring baseline).
    pub confidence: Option<i64>,
    /// Structured challenge — required in round 2, optional elsewhere.
    pub challenge: Option<ChallengeField>,
    /// Position-change declaration — required in round 4, optional elsewhere.
    pub position_change: Option<PositionChangeField>,
}

/// Send a Phase 1 debate round request to a bot.
///
/// Enforces a response body size limit to prevent DoS from oversized payloads.
pub async fn send_debate_request(
    client: &ClientWithMiddleware,
    endpoint_url: &str,
    token: &str,
    request: &DebateRoundRequest,
) -> Result<DebateRoundResponse, String> {
    let resp = client
        .post(endpoint_url)
        .bearer_auth(token)
        .json(request)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("bot returned HTTP {}", resp.status()));
    }
    let body = resp.bytes().await
        .map_err(|e| format!("failed to read response body: {e}"))?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(format!(
            "response body too large: {} bytes (limit {})",
            body.len(), MAX_RESPONSE_BYTES
        ));
    }
    serde_json::from_slice::<DebateRoundResponse>(&body)
        .map_err(|e| format!("invalid response body: {e}"))
}
