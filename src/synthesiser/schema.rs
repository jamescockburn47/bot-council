use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The rigid output schema for Opus synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisOutput {
    /// The debate topic.
    pub topic: String,
    /// Points on which all participants explicitly agreed.
    pub consensus_points: Vec<ConsensusPoint>,
    /// Issues that remained unresolved at the end of the debate.
    pub live_disagreements: Vec<LiveDisagreement>,
    /// Position shifts identified as inadequately justified.
    pub flagged_capitulations: Vec<FlaggedCapitulation>,
    /// Minority positions preserved with full dignity.
    pub minority_positions: Vec<MinorityPosition>,
    /// Per-pseudonym confidence values across rounds 0–4 (None = absent).
    pub confidence_trajectories: HashMap<String, Vec<Option<i64>>>,
    /// High-level meta-observations (deterministic evidence-grounded narrative).
    pub meta_observations: String,
}

/// A point of consensus with supporting evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusPoint {
    /// The agreed point.
    pub point: String,
    /// Pseudonyms of bots that supported this point.
    pub supporting_bots: Vec<String>,
    /// Evidence with citations.
    pub evidence: String,
}

/// A live disagreement with two sides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDisagreement {
    /// The issue in dispute.
    pub issue: String,
    /// One side of the disagreement.
    pub side_a: DisagreementSide,
    /// The opposing side.
    pub side_b: DisagreementSide,
}

/// One side of a disagreement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisagreementSide {
    /// The position held.
    pub position: String,
    /// Pseudonyms of bots holding this position.
    pub bots: Vec<String>,
    /// The strongest argument offered, with citation.
    pub best_argument: String,
}

/// A flagged capitulation (position change without adequate justification).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlaggedCapitulation {
    /// Pseudonym of the bot that changed position.
    pub bot: String,
    /// The prior position.
    pub from: String,
    /// The new position.
    pub to: String,
    /// Whether the stated justification was adequate.
    pub justification_adequate: bool,
    /// Reason for the flag.
    pub flag_reason: String,
}

/// A minority position preserved in the synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinorityPosition {
    /// Pseudonym of the bot holding this position.
    pub bot: String,
    /// The position itself.
    pub position: String,
    /// The strongest argument offered, with citation.
    pub key_argument: String,
    /// Confidence level at the end of the debate, if provided.
    pub confidence: Option<i64>,
}
