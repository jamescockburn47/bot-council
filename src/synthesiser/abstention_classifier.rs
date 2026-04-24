//! LLM-backed classifier for effective abstentions.
//!
//! The regex-based `is_effective_abstention_response` in
//! `orchestrator::multi_round` catches a fixed list of markers; in practice
//! bot wrappers improvise (e.g. "I could not complete the upstream model
//! call", "provider-failure notice"). The live flow still uses the regex
//! for speed, but before synthesis we run every response through this
//! model-backed classifier so the synthesiser gets the correct
//! effective_abstained flag for each round.
//!
//! The classifier is infallible from the caller's point of view: on LLM
//! failure it falls back to the cheap regex decision rather than
//! propagating an error and tanking synthesis.

use crate::config::ModelsConfig;
use crate::orchestrator::multi_round::is_effective_abstention_response;
use crate::synthesiser::{
    LocalChatCompletionRequest, LocalChatMessage, LocalResponseFormat, call_model_json,
};
use futures::future::join_all;
use serde::Deserialize;

/// Result of classifying a single response.
#[derive(Debug, Clone)]
pub struct AbstentionClassification {
    pub effective_abstention: bool,
    /// Short human-readable explanation; empty when the regex fast-path
    /// decided without an LLM call.
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct ClassifierJson {
    effective_abstention: bool,
    #[serde(default)]
    reason: String,
}

/// Hard cap on classifier input length. Keeps the prompt small and the
/// LLM call cheap — long responses' first 4 kB is plenty to judge
/// substance.
const MAX_CLASSIFY_CHARS: usize = 4096;

/// Tiny, fixed prompt. Substantive responses (even brief) stay FALSE;
/// only clear non-engagement is TRUE.
fn classifier_prompt(snippet: &str) -> String {
    format!(
        "You are classifying a single response from one participant in a multi-round debate. \
Return JSON and nothing else: {{\"effective_abstention\": boolean, \"reason\": \"under 20 words\"}}.\n\n\
effective_abstention is TRUE when the response does not engage with the debate — e.g. a provider or wrapper error notice (\"could not complete the upstream model call\", \"provider-failure notice\", \"model returned empty\"), an explicit refusal (\"I abstain\", \"I am unable\", \"cannot formulate\"), an empty or placeholder reply, or a purely procedural message with no argument.\n\n\
effective_abstention is FALSE when the response makes any substantive claim, argument, counter-argument, challenge, pointed question, or steelman about the debate topic — even briefly. Short substantive responses are NOT abstentions; length alone never decides. When in doubt, answer FALSE.\n\n\
RESPONSE:\n---\n{snippet}\n---"
    )
}

/// Classify one response. Always returns a value; falls back to the regex
/// decision if the LLM call or parse fails.
pub async fn classify_one(config: &ModelsConfig, text: &str) -> AbstentionClassification {
    let regex_decision = is_effective_abstention_response(text);

    // Short-circuit empty / trivially-short responses: the regex is
    // authoritative and cheap, don't spend an LLM call.
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return AbstentionClassification {
            effective_abstention: true,
            reason: "empty response".into(),
        };
    }

    let snippet: String = trimmed.chars().take(MAX_CLASSIFY_CHARS).collect();
    let request = LocalChatCompletionRequest {
        model: config.effective_final_synthesis_model().to_string(),
        temperature: 0.0,
        top_k: 1,
        seed: 42,
        cache_prompt: false,
        reasoning_format: "none".into(),
        response_format: LocalResponseFormat {
            format_type: "json_object".into(),
            schema: None,
        },
        messages: vec![LocalChatMessage {
            role: "user".into(),
            content: classifier_prompt(&snippet),
        }],
    };

    match call_model_json(config, &request, false).await {
        Ok(content) => match serde_json::from_str::<ClassifierJson>(&content) {
            Ok(parsed) => AbstentionClassification {
                effective_abstention: parsed.effective_abstention,
                reason: parsed.reason,
            },
            Err(e) => {
                tracing::warn!(error = %e, "abstention classifier returned unparseable JSON; using regex fallback");
                AbstentionClassification {
                    effective_abstention: regex_decision,
                    reason: String::new(),
                }
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "abstention classifier LLM call failed; using regex fallback");
            AbstentionClassification {
                effective_abstention: regex_decision,
                reason: String::new(),
            }
        }
    }
}

/// Classify a batch of responses in parallel. Output order matches input
/// order one-for-one.
pub async fn classify_batch(
    config: &ModelsConfig,
    texts: &[&str],
) -> Vec<AbstentionClassification> {
    let futures = texts.iter().map(|t| classify_one(config, t));
    join_all(futures).await
}
