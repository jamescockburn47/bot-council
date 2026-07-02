use serde::{Deserialize, Serialize};

/// Terminal artifact of a council session (debate today; competition and
/// research modes will emit the same shape). Issue-centric: every reader
/// surface — Layer 0 headline, issue cards, the map — renders from `issues`.
///
/// Every field carries `#[serde(default)]`: MiniMax-M2.7 sometimes drops
/// fields on shorter transcripts. Without defaults, one dropped field fails
/// the typed parse and the whole synthesis falls through to the
/// empty-template salvage; with defaults, only the dropped section is empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionArtifact {
    /// The session topic. `run_synthesis` reinjects the known topic before
    /// parsing, so the default rarely fires for this field.
    #[serde(default)]
    pub topic: String,
    /// Layer 0: one to two sentences stating the argument's end state
    /// across all issues — nuance-honest, never a verdict.
    #[serde(default)]
    pub headline: String,
    /// Four-sentence plain-prose outcome summary for a reader who has not
    /// followed the session. No pseudonyms, no citations.
    #[serde(default)]
    pub executive_summary: String,
    /// The argument's anatomy: one entry per distinct question at issue.
    #[serde(default)]
    pub issues: Vec<Issue>,
    /// High-level meta-observations (deterministic evidence-grounded
    /// narrative; composed council-side, not model-authored).
    #[serde(default)]
    pub meta_observations: String,
}

/// One question at issue in the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    /// The question, stated neutrally, full sentence.
    #[serde(default)]
    pub issue: String,
    /// 3–6 word keyword label (issue-card title, map anchor label).
    #[serde(default)]
    pub headline: String,
    /// True for the issue selected as the debate's crux between R2 and R3.
    /// At most one issue per artifact carries this.
    #[serde(default)]
    pub is_crux: bool,
    /// How the issue ended.
    #[serde(default)]
    pub status: IssueStatus,
    /// Surviving positions. One position => settled; two or more => split.
    /// A position with a single bot is a minority position by definition.
    #[serde(default)]
    pub positions: Vec<Position>,
    /// Position shifts observed on this issue across rounds.
    #[serde(default)]
    pub movement: Vec<Movement>,
}

/// How an issue ended. Unknown model output degrades to `Split` — the
/// honest default is "contested"; consensus is never manufactured by a
/// parse fallback.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
    /// One surviving position shared by all participating bots.
    Settled,
    /// The framing itself was rejected; the surviving contribution is a
    /// proposed reframing rather than a side.
    Reframed,
    /// Two or more positions remained live at close. Last variant on
    /// purpose: `#[serde(other)]` must sit on the final variant, so any
    /// unknown status string degrades here.
    #[default]
    #[serde(other)]
    Split,
}

/// One surviving position on an issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// The position held, full sentence.
    #[serde(default)]
    pub stance: String,
    /// 3–6 word keyword label (map node label).
    #[serde(default)]
    pub headline: String,
    /// Pseudonyms of bots holding this position at close.
    #[serde(default)]
    pub bots: Vec<String>,
    /// The strongest argument offered, with citation.
    #[serde(default)]
    pub best_argument: String,
    /// Evidence with citations.
    #[serde(default)]
    pub evidence: String,
    /// Mean end-of-session confidence of the holders, if reported.
    #[serde(default)]
    pub final_confidence: Option<i64>,
    /// True when this "position" rejects the issue's framing rather than
    /// taking a side. Rendered distinctly; never flattened into a pole.
    #[serde(default)]
    pub frame_rejection: bool,
}

/// A position shift observed on an issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Movement {
    /// Pseudonym of the bot that moved.
    #[serde(default)]
    pub bot: String,
    /// The prior position.
    #[serde(default)]
    pub from: String,
    /// The new position.
    #[serde(default)]
    pub to: String,
    /// False when the shift lacked adequate justification (the old
    /// "flagged capitulation"). Defaults TRUE — a dropped field must not
    /// falsely accuse a bot of capitulating.
    #[serde(default = "default_true")]
    pub justified: bool,
    /// Verbatim transcript quote that triggered or justified the shift.
    #[serde(default)]
    pub trigger_quote: String,
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_fixture() -> &'static str {
        r#"{
            "topic": "t",
            "headline": "Split 2-1 on enforcement; consensus on capture risk.",
            "executive_summary": "One. Two. Three. Four.",
            "issues": [
                {
                    "issue": "Whether enforcement should be ex-ante",
                    "headline": "Ex-ante enforcement viability",
                    "is_crux": true,
                    "status": "split",
                    "positions": [
                        {
                            "stance": "Ex-ante enforcement is workable",
                            "headline": "Ex-ante workable",
                            "bots": ["Agent A", "Agent B"],
                            "best_argument": "Precedent exists [Agent A, Round 2]",
                            "evidence": "Cited three regimes [Agent A, Round 2]",
                            "final_confidence": 70,
                            "frame_rejection": false
                        },
                        {
                            "stance": "The ex-ante/ex-post dichotomy is false",
                            "headline": "Dichotomy rejected",
                            "bots": ["Agent C"],
                            "best_argument": "Hybrid regimes dominate [Agent C, Round 3]",
                            "evidence": "",
                            "final_confidence": null,
                            "frame_rejection": true
                        }
                    ],
                    "movement": [
                        {
                            "bot": "Agent B",
                            "from": "Ex-post only",
                            "to": "Ex-ante workable",
                            "justified": true,
                            "trigger_quote": "the precedent Agent A cited"
                        }
                    ]
                }
            ],
            "meta_observations": "Conclusion: m"
        }"#
    }

    #[test]
    fn full_artifact_parses() {
        let a: SessionArtifact = serde_json::from_str(full_fixture()).unwrap();
        assert_eq!(a.issues.len(), 1);
        let issue = &a.issues[0];
        assert!(issue.is_crux);
        assert_eq!(issue.status, IssueStatus::Split);
        assert_eq!(issue.positions.len(), 2);
        assert!(issue.positions[1].frame_rejection);
        assert_eq!(issue.positions[0].final_confidence, Some(70));
        assert_eq!(issue.movement.len(), 1);
        assert!(issue.movement[0].justified);
    }

    #[test]
    fn dropped_fields_default_instead_of_failing() {
        // MiniMax sometimes drops whole fields; every field must default.
        let a: SessionArtifact =
            serde_json::from_str(r#"{"topic":"t","issues":[{"issue":"q"}]}"#).unwrap();
        assert_eq!(a.headline, "");
        assert_eq!(a.issues[0].status, IssueStatus::Split);
        assert!(a.issues[0].positions.is_empty());
        assert!(a.issues[0].movement.is_empty());
        // movement.justified defaults TRUE — never falsely flag a shift
        let m: Movement = serde_json::from_str(r#"{"bot":"A","from":"x","to":"y"}"#).unwrap();
        assert!(m.justified);
    }

    #[test]
    fn unknown_status_degrades_to_split_not_parse_failure() {
        // "contested"/"resolved"/typos must not nuke the whole parse.
        // Split is the honest default: unknown => treat as contested,
        // never manufacture consensus.
        let i: Issue = serde_json::from_str(r#"{"issue":"q","status":"contested"}"#).unwrap();
        assert_eq!(i.status, IssueStatus::Split);
    }

    #[test]
    fn legacy_shape_parses_as_empty_artifact() {
        // Old stored rows (pre-resynth) must not crash the typed parse —
        // they produce an artifact with zero issues, which downstream
        // renders as "synthesis not available / resynth needed".
        let legacy = r#"{"topic":"t","consensus_points":[{"point":"p"}],
            "live_disagreements":[],"minority_positions":[],
            "confidence_trajectories":{},"meta_observations":"m"}"#;
        let a: SessionArtifact = serde_json::from_str(legacy).unwrap();
        assert!(a.issues.is_empty());
        assert_eq!(a.topic, "t");
    }

    #[test]
    fn status_serialises_snake_case() {
        assert_eq!(
            serde_json::to_string(&IssueStatus::Reframed).unwrap(),
            "\"reframed\""
        );
    }
}
