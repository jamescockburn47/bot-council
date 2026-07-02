use crate::bot_client::{self, DebateRoundRequest, DebateRoundResponse};
use crate::orchestrator::multi_round::is_effective_abstention_response;
use reqwest_middleware::ClientWithMiddleware;
use std::time::Duration;

/// Outcome of dispatching one round request to a bot.
/// `retry_count` and `fallback_from_round` are persisted to the
/// `responses` row by the caller.
///
/// `PartialEq` is intentionally not derived: the inner
/// `DebateRoundResponse` (from `bot_client`) does not implement it and
/// adding it there is out of scope for this helper.
#[derive(Debug, Clone)]
pub enum DispatchOutcome {
    /// Bot responded successfully on the first attempt or the retry.
    Success {
        response: DebateRoundResponse,
        retry_count: u32,
    },
    /// Both attempts failed; the bot's round-0 text is carried forward
    /// so its voice is not silenced for the remainder of the debate.
    CarriedForward { r0_text: String, retry_count: u32 },
    /// Round 0 itself was not available; the bot is genuinely abstained
    /// for this round. The caller should set `abstained = true` on the
    /// response row.
    Abstained { retry_count: u32 },
}

/// Dispatch a round request with one retry and R0-carry-forward fallback.
///
/// Sequence:
/// 1. Fire `req` with the original prompt, `timeout_secs` budget.
/// 2. If the attempt fails (HTTP error, timeout, stock abstention text,
///    or caller-defined structural invalidity), re-fire with a
///    simplified retry prompt at the same timeout budget.
/// 3. If the retry also fails and `r0_text` is `Some`, carry it forward.
/// 4. Otherwise, mark genuinely abstained.
///
/// `is_structurally_invalid` lets callers apply round-specific
/// validation (for example, R2 requires a `challenge` field) and treat
/// structural failure as a retry trigger. Pass `|_| false` when no
/// round-specific validation is required.
pub async fn dispatch_with_retry_and_fallback(
    client: &ClientWithMiddleware,
    bot_kind: &str,
    endpoint: &str,
    token: &str,
    req: &DebateRoundRequest,
    retry_prompt: String,
    r0_text: Option<String>,
    timeout_secs: u64,
    is_structurally_invalid: impl Fn(&DebateRoundResponse) -> bool,
) -> DispatchOutcome {
    let first = try_dispatch(client, bot_kind, endpoint, token, req, timeout_secs).await;
    if let Some(r) = first {
        if !is_effective_abstention_response(&r.response) && !is_structurally_invalid(&r) {
            return DispatchOutcome::Success {
                response: r,
                retry_count: 0,
            };
        }
    }

    // DebateRoundRequest does not derive Clone (see bot_client/mod.rs),
    // so rebuild the retry request explicitly from the original fields.
    let retry_req = DebateRoundRequest {
        session_id: req.session_id.clone(),
        round: req.round,
        role: req.role.clone(),
        context: req.context.clone(),
        prompt: retry_prompt,
    };
    let second = try_dispatch(client, bot_kind, endpoint, token, &retry_req, timeout_secs).await;
    if let Some(r) = second {
        if !is_effective_abstention_response(&r.response) && !is_structurally_invalid(&r) {
            return DispatchOutcome::Success {
                response: r,
                retry_count: 1,
            };
        }
    }

    match r0_text {
        Some(text) => DispatchOutcome::CarriedForward {
            r0_text: text,
            retry_count: 1,
        },
        None => DispatchOutcome::Abstained { retry_count: 1 },
    }
}

async fn try_dispatch(
    client: &ClientWithMiddleware,
    bot_kind: &str,
    endpoint: &str,
    token: &str,
    req: &DebateRoundRequest,
    timeout_secs: u64,
) -> Option<DebateRoundResponse> {
    match tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        bot_client::dispatch_round_request(client, bot_kind, endpoint, token, req),
    )
    .await
    {
        Ok(Ok(resp)) => Some(resp),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "dispatch_with_retry_and_fallback: request failed");
            None
        }
        Err(_) => {
            tracing::warn!("dispatch_with_retry_and_fallback: request timed out");
            None
        }
    }
}

/// Standardised retry prompt injected on second attempt.
///
/// Kept deliberately simple — the goal is to unstick a bot that failed
/// the first attempt for a transient or structural reason, not to
/// coerce a particular round-specific answer.
pub fn simplified_retry_prompt(topic: &str, round_number: i64) -> String {
    format!(
        "Answer this round in one paragraph using your prior-round position as a \
         starting point. If you genuinely cannot, reply with one sentence \
         explaining why.\n\n\
         Topic: {topic}\n\
         Current round: {round_number}."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_variants_cover_three_cases() {
        let _ok = DispatchOutcome::Success {
            response: DebateRoundResponse {
                response: "hi".into(),
                confidence: None,
                challenge: None,
                position_change: None,
                ingest_kind: Default::default(),
            },
            retry_count: 0,
        };
        let _cf = DispatchOutcome::CarriedForward {
            r0_text: "original".into(),
            retry_count: 1,
        };
        let _abs = DispatchOutcome::Abstained { retry_count: 1 };
    }

    #[test]
    fn simplified_retry_prompt_includes_topic_and_round() {
        let p = simplified_retry_prompt("foo", 3);
        assert!(p.contains("foo"));
        assert!(p.contains("Current round: 3"));
    }
}
