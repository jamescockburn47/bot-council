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
