//! Debate lifecycle events for SSE streaming.

use serde::Serialize;

/// A debate lifecycle event, emitted by the orchestrator and consumed by SSE clients.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum DebateEvent {
    #[serde(rename = "debate:started")]
    DebateStarted { debate_id: String, topic: String },

    #[serde(rename = "round:started")]
    RoundStarted { round_number: i64, name: String },

    #[serde(rename = "response:received")]
    ResponseReceived {
        round_number: i64,
        pseudonym: String,
        role: String,
        response: String,
        confidence: Option<i64>,
        challenge: Option<serde_json::Value>,
        position_change: Option<serde_json::Value>,
        valid: bool,
        abstained: bool,
    },

    #[serde(rename = "round:completed")]
    RoundCompleted {
        round_number: i64,
        response_count: usize,
        valid_count: usize,
    },

    #[serde(rename = "synthesis:started")]
    SynthesisStarted,

    #[serde(rename = "synthesis:completed")]
    SynthesisCompleted {
        synthesis: serde_json::Value,
        citation_check: Option<serde_json::Value>,
    },

    #[serde(rename = "debate:completed")]
    DebateCompleted,

    #[serde(rename = "debate:failed")]
    DebateFailed { reason: String },
}

impl DebateEvent {
    /// The SSE event type string (used in `event:` field).
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::DebateStarted { .. } => "debate:started",
            Self::RoundStarted { .. } => "round:started",
            Self::ResponseReceived { .. } => "response:received",
            Self::RoundCompleted { .. } => "round:completed",
            Self::SynthesisStarted => "synthesis:started",
            Self::SynthesisCompleted { .. } => "synthesis:completed",
            Self::DebateCompleted => "debate:completed",
            Self::DebateFailed { .. } => "debate:failed",
        }
    }
}

/// Round number to human-readable name.
pub fn round_name(round: i64) -> &'static str {
    match round {
        0 => "Blind Formation",
        1 => "Anonymous Distribution",
        2 => "Structured Rebuttal",
        3 => "Cross-Examination",
        4 => "Final Position",
        _ => "Unknown",
    }
}
