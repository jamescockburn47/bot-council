//! Text-only bot mode dispatch.
//!
//! Contract: POST {url} with Authorization: Bearer {token} and body
//! `{prompt, session_id}`, expect `{text}` back. No round-specific fields,
//! no structured output. The response is translated into a `DebateRoundResponse`
//! with only `response` populated; all structured fields are left None.

use super::DebateRoundResponse;
use super::ingest;
use reqwest_middleware::ClientWithMiddleware;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct TextOnlyRequest<'a> {
    session_id: &'a str,
    prompt: &'a str,
}

/// Send a text-only prompt to a bot and translate the response into the
/// shared `DebateRoundResponse` type. Response parsing is lenient (the
/// ingest ladder): anything prose-shaped counts; oversize is truncated,
/// never rejected. Structured fields are always None; post-round
/// extraction populates them when required.
pub async fn send_text_only_request(
    client: &ClientWithMiddleware,
    endpoint_url: &str,
    token: &str,
    session_id: &str,
    prompt: &str,
) -> Result<DebateRoundResponse, String> {
    let mut req = client.post(endpoint_url);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let body = TextOnlyRequest { session_id, prompt };
    let resp = req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("bot returned HTTP {status}"));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("failed to read response body: {e}"))?;
    let ingested = ingest::ingest_prose(&bytes);
    Ok(DebateRoundResponse {
        response: ingested.text,
        confidence: None,
        challenge: None,
        position_change: None,
        ingest_kind: ingested.kind,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_client::build_http_client;
    use crate::config::HttpClientConfig;
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_http_config() -> HttpClientConfig {
        HttpClientConfig {
            connect_timeout_secs: 2,
            request_timeout_secs: 5,
            max_retries: 0,
            retry_delay_secs: 1,
        }
    }

    #[tokio::test]
    async fn happy_path_returns_text_as_response() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("authorization", "Bearer secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "text": "My position is X."
            })))
            .mount(&server)
            .await;
        let client = build_http_client(&test_http_config());
        let out =
            send_text_only_request(&client, &server.uri(), "secret", "sess-1", "Prompt").await;
        let resp = out.unwrap();
        assert_eq!(resp.response, "My position is X.");
        assert!(resp.challenge.is_none());
        assert!(resp.position_change.is_none());
        assert!(resp.confidence.is_none());
    }

    #[tokio::test]
    async fn http_error_is_propagated() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;
        let client = build_http_client(&test_http_config());
        let out = send_text_only_request(&client, &server.uri(), "", "sess-1", "Prompt").await;
        assert!(out.is_err());
        assert!(out.unwrap_err().contains("HTTP"));
    }

    #[tokio::test]
    async fn raw_text_body_is_salvaged() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json but still prose"))
            .mount(&server)
            .await;
        let client = build_http_client(&test_http_config());
        let out = send_text_only_request(&client, &server.uri(), "", "sess-1", "Prompt").await;
        let resp = out.unwrap();
        assert_eq!(resp.response, "not json but still prose");
        assert_eq!(resp.ingest_kind, ingest::IngestKind::SalvagedRaw);
    }

    #[tokio::test]
    async fn response_field_accepted_as_clean() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response": "external-shape prose"
            })))
            .mount(&server)
            .await;
        let client = build_http_client(&test_http_config());
        let out = send_text_only_request(&client, &server.uri(), "", "sess-1", "Prompt").await;
        let resp = out.unwrap();
        assert_eq!(resp.response, "external-shape prose");
        assert_eq!(resp.ingest_kind, ingest::IngestKind::Clean);
    }

    #[tokio::test]
    async fn oversize_body_truncates_never_rejects() {
        let server = MockServer::start().await;
        let big = "y".repeat(2 * crate::sanitise::MAX_RESPONSE_BYTES);
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(big))
            .mount(&server)
            .await;
        let client = build_http_client(&test_http_config());
        let out = send_text_only_request(&client, &server.uri(), "", "sess-1", "Prompt").await;
        let resp = out.unwrap();
        assert_eq!(resp.ingest_kind, ingest::IngestKind::Truncated);
        assert!(!resp.response.is_empty());
        assert!(resp.response.len() <= crate::sanitise::MAX_RESPONSE_BYTES);
    }
}
