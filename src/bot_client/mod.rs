use crate::config::HttpClientConfig;
use crate::sanitise::MAX_RESPONSE_BYTES;
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod position_scoring;
pub mod text_only;
pub use position_scoring::{
    PositionRequest, PositionResponse, ScoreEntry, ScoringContext, ScoringRequest, ScoringResponse,
    send_position_request, send_scoring_request,
};
pub use text_only::send_text_only_request;

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
///
/// Enforces a response body size limit to prevent DoS from oversized payloads.
pub async fn send_debate_request(
    client: &ClientWithMiddleware,
    endpoint_url: &str,
    token: &str,
    request: &DebateRoundRequest,
) -> Result<DebateRoundResponse, String> {
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
    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("failed to read response body: {e}"))?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(format!(
            "response body too large: {} bytes (limit {})",
            body.len(),
            MAX_RESPONSE_BYTES
        ));
    }
    serde_json::from_slice::<DebateRoundResponse>(&body)
        .map_err(|e| format!("invalid response body: {e}"))
}

/// Dispatch a debate round request to a bot, routing based on `bot_kind`.
///
/// - `"external"` (default): existing `/debate` contract via `send_debate_request`.
/// - `"text_only"`: minimal `/hook` contract via `send_text_only_request`;
///   `request.session_id` and `request.prompt` are used, and the structured
///   output fields come back as None.
///
/// Unknown kinds are treated as errors to fail loudly if a new kind is
/// added elsewhere without updating this dispatcher.
pub async fn dispatch_round_request(
    client: &ClientWithMiddleware,
    bot_kind: &str,
    endpoint_url: &str,
    token: &str,
    request: &DebateRoundRequest,
) -> Result<DebateRoundResponse, String> {
    match bot_kind {
        "external" => send_debate_request(client, endpoint_url, token, request).await,
        "text_only" => {
            text_only::send_text_only_request(
                client,
                endpoint_url,
                token,
                &request.session_id,
                &request.prompt,
            )
            .await
        }
        other => Err(format!("unknown bot_kind: {other}")),
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn cfg() -> crate::config::HttpClientConfig {
        crate::config::HttpClientConfig {
            connect_timeout_secs: 2,
            request_timeout_secs: 5,
            max_retries: 0,
            retry_delay_secs: 1,
        }
    }

    fn round_request() -> DebateRoundRequest {
        DebateRoundRequest {
            session_id: "s1".into(),
            round: 0,
            role: "proponent".into(),
            context: vec![],
            prompt: "Make your case.".into(),
        }
    }

    #[tokio::test]
    async fn external_kind_uses_full_contract() {
        let server = MockServer::start().await;
        // The external contract sends role/context/round — assert the body shape.
        Mock::given(method("POST"))
            .and(body_partial_json(serde_json::json!({
                "session_id": "s1", "round": 0, "role": "proponent"
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": "answer"
            })))
            .mount(&server)
            .await;
        let client = build_http_client(&cfg());
        let resp = dispatch_round_request(&client, "external", &server.uri(), "", &round_request())
            .await
            .unwrap();
        assert_eq!(resp.response, "answer");
    }

    #[tokio::test]
    async fn text_only_kind_uses_minimal_contract() {
        let server = MockServer::start().await;
        // The text_only contract sends only session_id + prompt — body must not
        // contain round/role/context keys.
        Mock::given(method("POST"))
            .and(body_partial_json(serde_json::json!({
                "session_id": "s1", "prompt": "Make your case."
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "answer"
            })))
            .mount(&server)
            .await;
        let client = build_http_client(&cfg());
        let resp =
            dispatch_round_request(&client, "text_only", &server.uri(), "", &round_request())
                .await
                .unwrap();
        assert_eq!(resp.response, "answer");
    }

    #[tokio::test]
    async fn unknown_kind_errors() {
        let client = build_http_client(&cfg());
        let out =
            dispatch_round_request(&client, "wizard", "http://unused", "", &round_request()).await;
        assert!(out.unwrap_err().contains("unknown bot_kind"));
    }
}
