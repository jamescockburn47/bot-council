use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::config::HttpClientConfig;

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundContext {
    pub pseudonym: String,
    pub round: i64,
    pub response: String,
    pub confidence: Option<i64>,
}

/// Phase 1 request payload for all rounds. Superset of Phase 0 PositionRequest.
#[derive(Debug, Serialize)]
pub struct DebateRoundRequest {
    pub session_id: String,
    pub round: i64,
    pub role: String,
    pub context: Vec<RoundContext>,
    pub prompt: String,
}

/// Structured challenge object (Round 2 required, optional other rounds).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeField {
    pub claim_targeted: String,
    pub counter_evidence: String,
    #[serde(rename = "type")]
    pub challenge_type: String,
}

/// Position change declaration (Round 4 required).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionChangeField {
    pub changed: bool,
    pub from_summary: String,
    pub to_summary: String,
    pub reason: String,
}

/// Phase 1 response from a bot. All fields after `response` are optional
/// depending on the round.
#[derive(Debug, Clone, Deserialize)]
pub struct DebateRoundResponse {
    pub response: String,
    pub confidence: Option<i64>,
    pub challenge: Option<ChallengeField>,
    pub position_change: Option<PositionChangeField>,
}

/// Send a Phase 1 debate round request to a bot.
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
    resp.json::<DebateRoundResponse>()
        .await
        .map_err(|e| format!("invalid response body: {e}"))
}
