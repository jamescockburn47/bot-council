# Issue-Centric Artifact Schema (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the five-list `SynthesisOutput` with the issue-centric `SessionArtifact` (spec: `docs/superpowers/specs/2026-07-02-issue-centric-sessions-design.md` Part 1), rewrite the synthesis prompt to emit it, thread the crux into the structured output, and keep every existing salvage/fallback safety net.

**Architecture:** All typed-schema knowledge lives in `src/synthesiser/`; the rest of the system passes synthesis JSON through as an opaque string (verified: only `schema.rs`, `mod.rs`, `citation_check.rs` touch the types; `resynth.rs` and `orchestrator/multi_round.rs` call `run_synthesis` and store the string). `mod.rs` is 1759 lines — far over the 300-line house cap — so this plan splits it into `client.rs` (HTTP), `evidence.rs` (transcript/grounding parsing), `prompt.rs` (prompt building), `meta.rs` (meta composition), leaving `mod.rs` as the pipeline.

**Tech Stack:** Rust 2024, serde, wiremock (existing test deps). All `cargo` commands run on EVO via `./scripts/sync-evo.sh` — the crate does not build on Windows.

**Deployment gate (BINDING):** This PR merges to `main` but is NOT shipped to prod alone — the live frontend reads the old payload shape. `./scripts/ship.sh` runs only after the Phase 2 frontend PR merges, then `ssh evo "bash /home/james/resynth-launch.sh"` rebuilds historical debates (operational lesson 16).

---

## Working notes for the implementer

- Branch: `git fetch origin && git switch -c claude/issue-centric-schema origin/main` (check `gh pr list --state open` first — lesson 10).
- Test command for one module: `./scripts/sync-evo.sh` runs `cargo test`; for fast iteration use `ssh -i ~/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test synthesiser"` after an `scp` sync (or just rerun `sync-evo.sh`, it syncs then tests).
- `sync-evo.sh check` = `cargo check --tests` for compile-only iterations.
- Every task ends with a commit; keep them atomic.
- The wiremock discriminator: integration tests match the synthesis call by a string that appears ONLY in the synthesis prompt. Today that string is `minority_positions`. The new prompt kills it; the new discriminator is `is_crux` (verified absent from divergence, extraction, crux-selector, and validator prompts — `crux_shift` and `justification_adequate` appear in divergence prompts, do NOT use those).

---

### Task 1: Rewrite `src/synthesiser/schema.rs` with the SessionArtifact types

**Files:**
- Modify: `src/synthesiser/schema.rs` (full rewrite, replaces all existing types)

- [ ] **Step 1: Write the failing tests** — append this test module to the bottom of the new `schema.rs` (written together with the types in Step 2; the types don't exist yet so tests fail to compile first):

```rust
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
        let i: Issue =
            serde_json::from_str(r#"{"issue":"q","status":"contested"}"#).unwrap();
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
```

- [ ] **Step 2: Write the types** — replace the entire contents of `src/synthesiser/schema.rs` above the test module with:

```rust
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
    /// Two or more positions remained live at close.
    #[default]
    #[serde(other)]
    Split,
    /// The framing itself was rejected; the surviving contribution is a
    /// proposed reframing rather than a side.
    Reframed,
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
```

- [ ] **Step 3: Verify only this module fails to compile elsewhere**

Run: `./scripts/sync-evo.sh check`
Expected: `schema.rs` compiles; `mod.rs` and `citation_check.rs` fail with unresolved imports (`SynthesisOutput`, `ConsensusPoint`, …). That's the expected blast radius — do NOT fix them yet; the compile break is resolved within this task sequence (lesson 1: keep it short).

- [ ] **Step 4: Commit (schema only — allowed to break the crate for the next task's duration is NOT acceptable per lesson 1, so this commit happens only locally and the push waits until Task 5 restores green).** Skip `git push`; commit locally:

```bash
git add src/synthesiser/schema.rs
git commit -m "feat(synthesis): SessionArtifact issue-centric schema types"
```

### Task 2: Extract shared evidence parsing into `src/synthesiser/evidence.rs`

Mechanical move — no behaviour change, no new code beyond visibility.

**Files:**
- Create: `src/synthesiser/evidence.rs`
- Modify: `src/synthesiser/mod.rs`

- [ ] **Step 1: Create `evidence.rs`** and move these items from `mod.rs`, verbatim, changing visibility to `pub(crate)`:
  - `struct TranscriptEntry` (fields → `pub(crate)`)
  - `struct GroundingEvidenceEntry` (fields → `pub(crate)`)
  - `fn default_valid_true`
  - `fn parse_grounding_evidence` → `pub(crate) fn`
  - `fn parse_transcript_entries` → `pub(crate) fn`
  - `fn parse_participant_map` → `pub(crate) fn`
  - `fn summarize_for_meta` → `pub(crate) fn`
  - `fn would_exceed_budget` → `pub(crate) fn`

  File header: `//! Transcript / grounding-evidence parsing shared by prompt, meta, and pipeline.` Add `use regex::Regex; use serde::Deserialize; use std::collections::HashMap;` as needed.

- [ ] **Step 2: Wire into `mod.rs`**: add `mod evidence;` and `use evidence::{parse_grounding_evidence, parse_transcript_entries, parse_participant_map, summarize_for_meta, would_exceed_budget, GroundingEvidenceEntry, TranscriptEntry};`. Delete the moved items from `mod.rs`. The two tests `derive_position_narrative_*` reference these transitively — leave the tests in `mod.rs` for now.

- [ ] **Step 3: Check compile state**

Run: `./scripts/sync-evo.sh check`
Expected: same residual errors as Task 1 Step 3 (old type names in `mod.rs`/`citation_check.rs`), NO new errors mentioning `evidence`.

- [ ] **Step 4: Commit locally**

```bash
git add src/synthesiser/evidence.rs src/synthesiser/mod.rs
git commit -m "refactor(synthesis): extract evidence parsing into evidence.rs"
```

### Task 3: Extract the HTTP client into `src/synthesiser/client.rs`

Mechanical move — no behaviour change.

**Files:**
- Create: `src/synthesiser/client.rs`
- Modify: `src/synthesiser/mod.rs`

- [ ] **Step 1: Create `client.rs`** and move verbatim from `mod.rs`:
  - `const LOCAL_SYNTHESIS_MAX_RETRIES`
  - `struct LocalChatCompletionRequest`, `LocalChatMessage`, `LocalChatCompletionResponse`, `LocalChatChoice`, `LocalChatChoiceMessage`, `LocalResponseFormat` (keep `pub(crate)` where already so)
  - `async fn call_local_synthesis_model` → `pub(crate)`
  - `pub(crate) async fn call_model_json`
  - `fn build_chat_completions_url`, `fn clean_model_output`, `fn extract_json_object` → `pub(crate) fn extract_json_object` (the `extract_json_object_handles_channel_wrapped_output` test moves with it into a `#[cfg(test)] mod tests` at the bottom of `client.rs`; update its fixture JSON in Task 7).

  File header: `//! OpenAI-compatible chat client for the final-synthesis model.` Imports: `use crate::config::ModelsConfig; use serde::{Deserialize, Serialize}; use std::time::Duration;`.

- [ ] **Step 2: Wire into `mod.rs`**: `mod client;` + `use client::{call_local_synthesis_model, call_model_json, LocalChatCompletionRequest, LocalChatMessage, LocalResponseFormat};` (the warmup fn uses the request types). Delete moved items from `mod.rs`.

- [ ] **Step 3: Check** — `./scripts/sync-evo.sh check`; expected: same residual old-type errors only.

- [ ] **Step 4: Commit locally**

```bash
git add src/synthesiser/client.rs src/synthesiser/mod.rs
git commit -m "refactor(synthesis): extract model HTTP client into client.rs"
```

### Task 4: New prompt in `src/synthesiser/prompt.rs`

**Files:**
- Create: `src/synthesiser/prompt.rs`
- Modify: `src/synthesiser/mod.rs`

- [ ] **Step 1: Write the failing tests** — bottom of the new `prompt.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::build_synthesis_prompt;
    use crate::analyser::crux::CruxSelection;

    #[test]
    fn prompt_contains_issue_schema_and_contract() {
        let p = build_synthesis_prompt("topic", "Agent A = Alice", "transcript", "{}", "[]", "[]", None);
        assert!(p.contains("STRICT OUTPUT CONTRACT"));
        assert!(p.contains("\"issues\""));
        assert!(p.contains("\"is_crux\""));
        assert!(p.contains("\"frame_rejection\""));
        assert!(p.contains("\"movement\""));
        assert!(p.contains("meta_observations"));
        // Old shape must be gone
        assert!(!p.contains("consensus_points"));
        assert!(!p.contains("confidence_trajectories"));
    }

    #[test]
    fn prompt_keeps_headline_and_summary_rules() {
        let p = build_synthesis_prompt("topic", "Agent A = Alice", "transcript", "{}", "[]", "[]", None);
        assert!(p.contains("HEADLINE RULES"));
        assert!(p.contains("EXECUTIVE_SUMMARY requirements"));
        assert!(p.contains("ABSTENTION HANDLING"));
    }

    #[test]
    fn crux_section_demands_single_crux_issue() {
        let crux = CruxSelection {
            claim: "SOC 2 costs are trivial".into(),
            source_pseudonym: "Agent A".into(),
            source_quote: "$30-80k for SOC 2 Type II".into(),
        };
        let p = build_synthesis_prompt("topic", "Agent A = Alice", "transcript", "{}", "[]", "[]", Some(&crux));
        assert!(p.contains("Crux outcome"));
        assert!(p.contains("SOC 2 costs are trivial"));
        assert!(p.contains("exactly one issue"));
        assert!(p.contains("crux_shift"));
    }

    #[test]
    fn crux_section_omitted_when_absent() {
        let p = build_synthesis_prompt("topic", "Agent A = Alice", "transcript", "{}", "[]", "[]", None);
        assert!(!p.contains("Crux outcome"));
    }
}
```

- [ ] **Step 2: Write the prompt module.** Move `derive_abstention_summary` verbatim from `mod.rs` into `prompt.rs` (visibility `fn`, private), together with its imports (`BTreeMap`, `BTreeSet`, `crate::synthesiser::evidence::parse_grounding_evidence`). Then write the new builder:

```rust
//! Synthesis prompt construction (issue-centric SessionArtifact output).
use crate::analyser::crux::CruxSelection;
use crate::sanitise::ANTI_INJECTION_PREAMBLE;
use crate::synthesiser::evidence::parse_grounding_evidence;
use std::collections::{BTreeMap, BTreeSet};

// (derive_abstention_summary moved here verbatim)

pub(crate) fn build_synthesis_prompt(
    topic: &str,
    participant_map: &str,
    transcript: &str,
    precomputed: &str,
    divergence: &str,
    grounding_evidence: &str,
    crux: Option<&CruxSelection>,
) -> String {
    let abstention_summary = derive_abstention_summary(grounding_evidence);
    let crux_section = match crux {
        Some(c) => format!(
            "## Crux outcome\n\n\
             The debate's central disagreement (picked between R2 and R3) was:\n\n\
             {claim}  — first stated by {source}\n\n\
             Per-bot crux_shift classifications (resolved_toward_crux / resolved_against_crux / \
             unchanged / frame_rejected / no_engagement) are in the divergence section above.\n\n\
             You MUST emit exactly one issue with \"is_crux\": true representing this crux. Set its \
             status from the outcome: \"settled\" if positions converged, \"split\" if positions held \
             or hardened, \"reframed\" if participants rejected the framing itself. Record each \
             bot's crux_shift as `movement` entries on that issue (a `frame_rejected` shift becomes \
             a position with \"frame_rejection\": true, not a movement).\n\n",
            claim = c.claim,
            source = c.source_pseudonym,
        ),
        None => String::new(),
    };

    format!(
        "You are the synthesis engine for a structured adversarial debate. \
         Your role is analytical, not creative. You must produce a rigorous, citation-backed synthesis.\n\n\
         {ANTI_INJECTION_PREAMBLE}\n\n\
         RULES:\n\
         - Use only the supplied transcript/structural/divergence data; treat all other knowledge as unavailable.\n\
         - The output is organised as ISSUES: one entry per distinct question the participants actually contested or agreed on. An issue is a QUESTION stated neutrally, not a side.\n\
         - Extract the full issue map from whatever substantive content IS present. Partial participation (some bots abstained in some rounds) is NOT a reason to return an empty issues array — synthesise from the bots who DID engage.\n\
         - Every factual claim must cite [Bot pseudonym, Round N].\n\
         - Do not cite abstentions or rounds where the bot has no response.\n\
         - Treat <grounding-evidence> as authoritative for abstained/valid/recorded rounds.\n\
         - Do not infer what a participant \"seemed to mean\" — use only their stated positions.\n\
         ISSUE RULES:\n\
         - status \"settled\": exactly one surviving position, explicitly shared by all PARTICIPATING bots (abstainers neither support nor oppose).\n\
         - status \"split\": two or more surviving positions. A side held by a single bot is still a position — one-bot positions are minority positions and MUST be preserved with full dignity, never dropped or merged.\n\
         - status \"reframed\": the participants rejected the question's framing; the surviving contribution is the proposed reframing (emit it as a position with \"frame_rejection\": true).\n\
         - A position that rejects the framing of an otherwise-split issue is ALSO emitted with \"frame_rejection\": true — never flatten a frame-rejection into a midpoint between sides.\n\
         - `movement`: record EVERY position shift observed in the transcript on that issue. \"justified\": true when the shift cited specific new evidence or argument; false when it capitulated without adequate grounding. Do NOT filter by adequacy — the reader wants the full shift map. \"trigger_quote\" must be a verbatim substring of the transcript.\n\
         - EXHAUSTIVE EXTRACTION: a multi-round debate usually contains several distinct issues (mechanism vs evidence vs scope vs definitional framing). Emit ONE issue per distinct question — do NOT merge separate questions into one umbrella issue, and do NOT collapse a debate into a single issue unless it genuinely only had one.\n\
         - TARGET COUNTS (guidance, not a floor): typically 3–6 issues for a healthy five-round debate; each split issue typically has 2–3 positions.\n\
         - Never decline to synthesise because you judged the evidence \"too limited\". If the transcript contains substantive bot responses, `issues` MUST be populated.\n\n\
         STRICT OUTPUT CONTRACT:\n\
         - Return exactly one valid JSON object. No markdown, no code fences, no prose outside JSON.\n\
         - Use only pseudonyms from <participant-map> in bots/bot fields.\n\
         - Keep evidence and best_argument short and specific (one claim + citation).\n\
         - An empty `movement` array is acceptable only if no bot shifted position on that issue. An empty `issues` array is acceptable ONLY for a genuinely contentless transcript.\n\
         - Do not include synthetic placeholders like \"TBD\", \"unknown source\", or uncited claims.\n\
         - meta_observations must start with \"Conclusion:\" then use these exact section headings and order: \"Summary of arguments\", \"Key disagreements\", \"Minority positions\", \"Overall outcome\", \"Bot behaviour notes\".\n\n\
         HEADLINE requirements (top-level field, 1–2 sentences):\n\
         - The state of the argument across ALL issues, for a reader deciding whether to read further. Status-honest: name what settled, what stayed split, and any standing reframing. NEVER a verdict; never smooth a split into a conclusion.\n\
         - No bot pseudonyms, no round numbers, no citations.\n\n\
         EXECUTIVE_SUMMARY requirements (plain prose, four sentences):\n\
         - Four full sentences of plain prose about the debate's OUTCOME on the TOPIC. No bullets, no lists, no headings.\n\
         - Tell a reader who has NOT followed the transcript where the debate landed: what was agreed, the central unresolved disagreement, and how the balance of argument fell.\n\
         - No bot pseudonyms, no round numbers, no bracketed citations, no confidence scores.\n\
         - Every sentence ends with terminal punctuation. No trailing ellipses or dangling clauses.\n\n\
         ABSTENTION HANDLING:\n\
         - The <abstention-summary> block names exactly which bots gapped which rounds. When writing meta_observations \"Bot behaviour notes\" and the outcome narrative, reflect these gaps accurately — never describe a bot as having argued, conceded, or proposed anything in a round they skipped.\n\
         - If a bot effectively abstained in a majority of rounds, say so in \"Bot behaviour notes\" and do not list that bot on positions formed in rounds they missed.\n\
         - When <abstention-summary> includes a verbatim self-reported signal for an abstaining bot, quote that signal directly in \"Bot behaviour notes\" (one sentence, in the format: `<Agent X> wrapper reported: \"<verbatim quote>\".`) and add one short operator-facing line suggesting where to look (e.g. \"Operator: check upstream model availability / API key / rate limits.\").\n\n\
         HEADLINE RULES (applies to every issue headline and position headline):\n\
         - `headline` is a graph-node label shown at normal zoom. It MUST be 3–6 words, keyword-style, no trailing punctuation.\n\
         - The headline is the SUBSTANCE of the question or claim — NOT meta-information about who agrees. Forbidden: \"All 4 participants agree\", \"Majority position\", \"Unanimous view\".\n\
         - Omit articles and filler. Use concrete nouns and verbs. Headlines must be mutually distinguishable at a glance.\n\
         - DO NOT truncate the full sentence into the headline. Write a fresh 3–6 word distillation.\n\
         - Good examples: \"Junior hiring collapses 30%\", \"Liability gap closable\", \"Ex-ante enforcement viability\", \"Dichotomy rejected\".\n\n\
         TOPIC: {topic}\n\n\
         <participant-map>\n{participant_map}\n</participant-map>\n\n\
         <abstention-summary>\n{abstention_summary}\n</abstention-summary>\n\n\
         <grounding-evidence>\n{grounding_evidence}\n</grounding-evidence>\n\n\
         <debate-transcript>\n{transcript}\n</debate-transcript>\n\n\
         <structural-data>\n{precomputed}\n</structural-data>\n\n\
         <divergence-analyses>\n{divergence}\n</divergence-analyses>\n\n\
         {crux_section}\
         OUTPUT SCHEMA (return valid JSON):\n\
         {{\n\
           \"topic\": \"string\",\n\
           \"headline\": \"1-2 sentences. State of the argument across all issues. No verdict.\",\n\
           \"executive_summary\": \"EXACTLY 4 full sentences. Plain prose. No bot names, no citations.\",\n\
           \"issues\": [{{\n\
             \"issue\": \"string — the question, stated neutrally\",\n\
             \"headline\": \"3-6 word label\",\n\
             \"is_crux\": bool,\n\
             \"status\": \"settled\" | \"split\" | \"reframed\",\n\
             \"positions\": [{{ \"stance\": \"string\", \"headline\": \"3-6 word label\", \"bots\": [\"pseudonym\"], \"best_argument\": \"string [citation]\", \"evidence\": \"string [citations]\", \"final_confidence\": int or null, \"frame_rejection\": bool }}],\n\
             \"movement\": [{{ \"bot\": \"pseudonym\", \"from\": \"string\", \"to\": \"string\", \"justified\": bool, \"trigger_quote\": \"verbatim transcript quote\" }}]\n\
           }}],\n\
           \"meta_observations\": \"string — target 350-700 words\"\n\
         }}"
    )
}
```

  Then in `mod.rs`: add `mod prompt;` + `use prompt::build_synthesis_prompt;`, delete the old `build_synthesis_prompt` and `derive_abstention_summary`, and delete the three old prompt tests (`synthesis_prompt_contains_strict_output_contract`, `synthesis_prompt_includes_crux_section_when_present`, `synthesis_prompt_omits_crux_section_when_absent`) — they are replaced by the `prompt.rs` tests above.

- [ ] **Step 3: Check** — `./scripts/sync-evo.sh check`; expected: remaining errors are confined to `mod.rs` pipeline functions and `citation_check.rs` (old type names), not `prompt.rs`.

- [ ] **Step 4: Commit locally**

```bash
git add src/synthesiser/prompt.rs src/synthesiser/mod.rs
git commit -m "feat(synthesis): issue-centric synthesis prompt in prompt.rs"
```

### Task 5: Rewrite the pipeline in `mod.rs` + meta composition in `meta.rs`

This task restores the crate to green.

**Files:**
- Create: `src/synthesiser/meta.rs`
- Modify: `src/synthesiser/mod.rs`

- [ ] **Step 1: Create `meta.rs`** with the meta-composition rewritten for `SessionArtifact`. Move `build_behavior_notes` and `derive_position_narrative` from `mod.rs` verbatim (they only touch evidence types — imports change to `crate::synthesiser::evidence::*`). Write the new composition:

```rust
//! Deterministic, evidence-grounded meta_observations composition.
use crate::synthesiser::evidence::{
    parse_grounding_evidence, parse_participant_map, parse_transcript_entries,
    summarize_for_meta, would_exceed_budget, GroundingEvidenceEntry,
};
use crate::synthesiser::schema::{IssueStatus, SessionArtifact};
use std::collections::HashMap;

/// Ensure meta observations are grounded in deterministic transcript evidence.
pub(crate) fn ensure_substantive_meta(
    output: &mut SessionArtifact,
    participant_map_text: &str,
    transcript_text: &str,
    grounding_evidence_json: &str,
) {
    output.meta_observations = compose_structured_meta(
        output,
        participant_map_text,
        transcript_text,
        grounding_evidence_json,
    );
    if !output
        .meta_observations
        .trim_start()
        .starts_with("Conclusion:")
    {
        output.meta_observations = format!("Conclusion: {}", output.meta_observations.trim());
    }
}

pub(crate) fn compose_structured_meta(
    output: &SessionArtifact,
    participant_map_text: &str,
    transcript_text: &str,
    grounding_evidence_json: &str,
) -> String {
    let mut sections = Vec::new();
    sections.push(format!("Conclusion: {}", derive_overall_outcome(output)));

    let settled: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.status == IssueStatus::Settled)
        .collect();
    let contested: Vec<_> = output
        .issues
        .iter()
        .filter(|i| i.status != IssueStatus::Settled)
        .collect();

    let summary_of_arguments = if output.issues.is_empty() {
        "- Structured issue map not extracted; see the raw transcript for per-bot positions."
            .to_string()
    } else {
        output
            .issues
            .iter()
            .take(6)
            .map(|i| {
                let status = match i.status {
                    IssueStatus::Settled => "settled",
                    IssueStatus::Split => "split",
                    IssueStatus::Reframed => "reframed",
                };
                let crux = if i.is_crux { " [crux]" } else { "" };
                format!(
                    "- {}{crux} ({status}): {}",
                    summarize_for_meta(&i.issue, 140),
                    i.positions
                        .iter()
                        .take(3)
                        .map(|p| format!(
                            "{} ({})",
                            summarize_for_meta(&p.stance, 100),
                            p.bots.join(", ")
                        ))
                        .collect::<Vec<_>>()
                        .join(" vs ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections.push(format!("Summary of arguments:\n{summary_of_arguments}"));

    let disagreements = if contested.is_empty() {
        "- No live disagreement remained at synthesis time.".to_string()
    } else {
        contested
            .iter()
            .take(4)
            .map(|i| {
                format!(
                    "- {} | {}",
                    summarize_for_meta(&i.issue, 90),
                    i.positions
                        .iter()
                        .take(3)
                        .map(|p| summarize_for_meta(&p.best_argument, 150))
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections.push(format!("Key disagreements:\n{disagreements}"));

    // Minority position = a position held by exactly one bot.
    let minorities: Vec<String> = output
        .issues
        .iter()
        .flat_map(|i| i.positions.iter())
        .filter(|p| p.bots.len() == 1)
        .take(4)
        .map(|p| {
            format!(
                "- {}: {} | {}",
                p.bots.first().cloned().unwrap_or_default(),
                summarize_for_meta(&p.stance, 120),
                summarize_for_meta(&p.best_argument, 180)
            )
        })
        .collect();
    let minority_section = if minorities.is_empty() {
        "- No explicit minority position was preserved in this run.".to_string()
    } else {
        minorities.join("\n")
    };
    sections.push(format!("Minority positions:\n{minority_section}"));

    let unjustified = output
        .issues
        .iter()
        .flat_map(|i| i.movement.iter())
        .filter(|m| !m.justified)
        .count();
    let outcome = format!(
        "- Issues: {} ({} settled, {} contested)\n- Position shifts: {} ({} flagged unjustified)",
        output.issues.len(),
        settled.len(),
        contested.len(),
        output
            .issues
            .iter()
            .map(|i| i.movement.len())
            .sum::<usize>(),
        unjustified
    );
    sections.push(format!("Overall outcome:\n{outcome}"));

    let behavior = build_behavior_notes(
        participant_map_text,
        transcript_text,
        grounding_evidence_json,
    );
    sections.push(format!("Bot behaviour notes:\n{behavior}"));

    sections.join("\n\n")
}

fn derive_overall_outcome(a: &SessionArtifact) -> String {
    let settled = a
        .issues
        .iter()
        .filter(|i| i.status == IssueStatus::Settled)
        .count();
    let contested = a.issues.len() - settled;
    match (settled, contested) {
        (0, 0) => "No structured issue map was extracted; the full per-bot positions remain in the transcript.".into(),
        (_, 0) => "Broad alignment: every extracted issue settled.".into(),
        (0, _) => "No consensus: every extracted issue remained contested at close.".into(),
        _ => "Partial convergence: some issues settled, others remained contested.".into(),
    }
}

// (build_behavior_notes and derive_position_narrative moved here verbatim)
```

- [ ] **Step 2: Rewrite the `mod.rs` pipeline.** With prompt/meta/client/evidence extracted, `mod.rs` keeps: `run_synthesis`, `wait_for_final_synthesis_ready`, `conservative_fallback`, `salvage_loose_output`, `enrich_empty_output_with_structural_fallback`, `ensure_crux_issue`. The changes:

  1. `use crate::synthesiser::schema::{Issue, IssueStatus, Position, SessionArtifact};` (drop old names). Module declarations: `pub mod abstention_classifier; pub mod citation_check; mod client; mod evidence; mod meta; pub mod precompute; mod prompt; pub mod schema;`
  2. In `run_synthesis`: replace `SynthesisOutput` with `SessionArtifact` throughout; the structurally-empty retry check becomes:

```rust
        let is_structurally_empty = attempt_parsed.issues.is_empty();
```

  3. `conservative_fallback` loses trajectories (schema has none) and its `precomputed_json` parameter:

```rust
/// Build a deterministic no-hallucination fallback with an empty issue map.
fn conservative_fallback(topic: &str) -> SessionArtifact {
    SessionArtifact {
        topic: topic.to_string(),
        headline: String::new(),
        executive_summary: String::new(),
        issues: Vec::new(),
        meta_observations: "Conservative fallback synthesis: no structured issue map is reported because the synthesis model output could not be validated against the required schema.".into(),
    }
}
```

  Update its three call sites in `run_synthesis`/`salvage_loose_output` to `conservative_fallback(topic)`.

  4. `salvage_loose_output`: same skeleton, plus salvage of the two new fields; the loose-issues parse relies on serde defaults:

```rust
    if let Some(h) = loose.get("headline").and_then(|v| v.as_str()) {
        if !h.trim().is_empty() {
            output.headline = h.trim().to_string();
        }
    }
    if let Some(issues_val) = loose.get("issues") {
        if let Ok(issues) = serde_json::from_value::<Vec<Issue>>(issues_val.clone()) {
            output.issues = issues;
        }
    }
```

  Delete the old consensus/disagreement string-join fallback for `meta` (those keys no longer exist in model output); keep the `meta_observations` salvage and the `ensure_substantive_meta` call (now `meta::ensure_substantive_meta`).

  5. Replace `enrich_empty_output_with_structural_minorities` with the issue-shaped fallback (same evidence-walk skeleton, new tail):

```rust
/// Last-resort hardening: when the model returns an empty issue map but the
/// transcript has substantive non-abstained content, emit one Split issue
/// holding each agent's latest substantive position. Ensures the map has
/// nodes even when extraction fails. No-op if `issues` is non-empty.
fn enrich_empty_output_with_structural_fallback(
    output: &mut SessionArtifact,
    transcript_text: &str,
    grounding_evidence_json: &str,
) {
    if !output.issues.is_empty() {
        return;
    }
    // ... identical evidence-collection walk as the old function
    // (parse_grounding_evidence, fallback to parse_transcript_entries,
    // latest_by_agent map, agents sorted) ...
    let mut positions = Vec::new();
    for agent in agents {
        let entry = &latest_by_agent[&agent];
        positions.push(Position {
            stance: evidence::summarize_for_meta(&entry.response, 280),
            headline: format!("{} position (R{})", agent, entry.round),
            bots: vec![agent.clone()],
            best_argument: format!(
                "Derived from [{}, Round {}] — no structured argument extracted by the synthesiser.",
                agent, entry.round
            ),
            evidence: String::new(),
            final_confidence: None,
            frame_rejection: false,
        });
    }
    if positions.is_empty() {
        return;
    }
    output.issues.push(Issue {
        issue: "Final positions on record (structural fallback — the synthesiser extracted no issue map)".into(),
        headline: "Positions on record".into(),
        is_crux: false,
        status: IssueStatus::Split,
        positions,
        movement: Vec::new(),
    });
    tracing::info!("synthesis: empty output enriched with structural fallback issue");
}
```

  6. New crux guarantee, called in `run_synthesis` after the enrich step and before `ensure_substantive_meta`:

```rust
/// Spec guarantee: when crux selection succeeded, the artifact carries
/// exactly one `is_crux` issue. If the model forgot the flag, mark the
/// issue with the highest word-overlap against the crux claim; if the
/// issue map is empty (or nothing overlaps), inject a positions-empty
/// crux issue so the reader still sees what the debate turned on.
fn ensure_crux_issue(output: &mut SessionArtifact, crux: Option<&crate::analyser::crux::CruxSelection>) {
    let Some(crux) = crux else { return };
    if output.issues.iter().any(|i| i.is_crux) {
        return;
    }
    fn words_of(s: &str) -> std::collections::HashSet<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 3)
            .map(str::to_string)
            .collect()
    }
    let claim_words = words_of(&crux.claim);
    let best = output
        .issues
        .iter_mut()
        .map(|i| {
            let overlap = words_of(&i.issue).intersection(&claim_words).count();
            (overlap, i)
        })
        .max_by_key(|(overlap, _)| *overlap);
    match best {
        Some((overlap, issue)) if overlap >= 2 => issue.is_crux = true,
        _ => output.issues.push(Issue {
            issue: crux.claim.clone(),
            headline: String::new(),
            is_crux: true,
            status: IssueStatus::Split,
            positions: Vec::new(),
            movement: Vec::new(),
        }),
    }
}
```

  Call order in `run_synthesis` (both the success path and NOT the conservative-fallback early-return, which stays as-is):

```rust
    enrich_empty_output_with_structural_fallback(&mut parsed, transcript_text, grounding_evidence_json);
    ensure_crux_issue(&mut parsed, crux);
    meta::ensure_substantive_meta(&mut parsed, participant_map_text, transcript_text, grounding_evidence_json);
```

- [ ] **Step 3: Update the `mod.rs` unit tests.**
  - `run_synthesis_accepts_null_minority_confidence` → rename `run_synthesis_accepts_null_position_confidence`; mock content becomes:

```rust
                        "content": serde_json::json!({
                            "topic": "t",
                            "headline": "One issue, one position.",
                            "executive_summary": "One. Two. Three. Four.",
                            "issues": [{
                                "issue": "q",
                                "headline": "Question label",
                                "is_crux": false,
                                "status": "split",
                                "positions": [{
                                    "stance": "p",
                                    "headline": "Stance label",
                                    "bots": ["Agent A"],
                                    "best_argument": "k [Agent A, Round 2]",
                                    "evidence": "",
                                    "final_confidence": null,
                                    "frame_rejection": false
                                }],
                                "movement": []
                            }],
                            "meta_observations": "m"
                        })
                        .to_string()
```

  - `structured_meta_leads_with_summary_sections` moves to `meta.rs` tests and is rebuilt against the new shape:

```rust
#[cfg(test)]
mod tests {
    use super::compose_structured_meta;
    use crate::synthesiser::schema::{Issue, IssueStatus, Position, SessionArtifact};

    #[test]
    fn structured_meta_leads_with_summary_sections() {
        let artifact = SessionArtifact {
            topic: "t".into(),
            headline: String::new(),
            executive_summary: String::new(),
            issues: vec![Issue {
                issue: "Whether identity certificates improve trust".into(),
                headline: "Certificate trust value".into(),
                is_crux: true,
                status: IssueStatus::Split,
                positions: vec![
                    Position {
                        stance: "Certificates materially improve trust".into(),
                        headline: "Certificates improve trust".into(),
                        bots: vec!["Agent A".into()],
                        best_argument: "Trust improves when attestations are verifiable [Agent A, Round 2]".into(),
                        evidence: String::new(),
                        final_confidence: Some(70),
                        frame_rejection: false,
                    },
                    Position {
                        stance: "Keep identity optional and audit controls mandatory".into(),
                        headline: "Audit over identity".into(),
                        bots: vec!["Agent C".into()],
                        best_argument: "Mandatory identity can be theatre without enforcement [Agent C, Round 2]".into(),
                        evidence: String::new(),
                        final_confidence: Some(62),
                        frame_rejection: false,
                    },
                ],
                movement: vec![],
            }],
            meta_observations: String::new(),
        };
        let evidence = serde_json::json!([
            {"agent":"Agent A","round":0,"abstained":false,"valid":true,"response":"opening"},
            {"agent":"Agent C","round":0,"abstained":false,"valid":true,"response":"opening"}
        ])
        .to_string();

        let meta = compose_structured_meta(&artifact, "Agent A = Alice\nAgent C = Cara", "", &evidence);
        assert!(meta.starts_with("Conclusion:"));
        assert!(meta.contains("Summary of arguments:"));
        assert!(meta.contains("[crux]"));
        assert!(meta.contains("Key disagreements:"));
        assert!(meta.contains("Minority positions:"));
        assert!(meta.contains("Audit over identity") || meta.contains("Agent C"));
        assert!(meta.contains("Overall outcome:"));
        assert!(meta.contains("Bot behaviour notes:"));
    }
}
```

  - The two `derive_position_narrative_*` tests and `derive_position_narrative_prefers_structured_grounding_evidence` move to `meta.rs` unchanged.
  - In `client.rs`, `extract_json_object_handles_channel_wrapped_output`'s fixture string becomes `{"topic":"t","headline":"h","executive_summary":"e","issues":[],"meta_observations":"m"}` (same assertion).
  - Add two new `mod.rs` tests:

```rust
    #[test]
    fn ensure_crux_issue_marks_best_overlap() {
        let crux = crate::analyser::crux::CruxSelection {
            claim: "Whether enforcement should be ex-ante rather than ex-post".into(),
            source_pseudonym: "Agent A".into(),
            source_quote: "q".into(),
        };
        let mut artifact = SessionArtifact {
            topic: "t".into(),
            headline: String::new(),
            executive_summary: String::new(),
            issues: vec![
                Issue { issue: "Whether capture risk is real".into(), headline: String::new(), is_crux: false, status: IssueStatus::Settled, positions: vec![], movement: vec![] },
                Issue { issue: "Whether enforcement should be ex-ante".into(), headline: String::new(), is_crux: false, status: IssueStatus::Split, positions: vec![], movement: vec![] },
            ],
            meta_observations: String::new(),
        };
        super::ensure_crux_issue(&mut artifact, Some(&crux));
        assert!(!artifact.issues[0].is_crux);
        assert!(artifact.issues[1].is_crux);
    }

    #[test]
    fn ensure_crux_issue_injects_when_no_overlap() {
        let crux = crate::analyser::crux::CruxSelection {
            claim: "Completely unrelated crux claim".into(),
            source_pseudonym: "Agent A".into(),
            source_quote: "q".into(),
        };
        let mut artifact = SessionArtifact {
            topic: "t".into(),
            headline: String::new(),
            executive_summary: String::new(),
            issues: vec![],
            meta_observations: String::new(),
        };
        super::ensure_crux_issue(&mut artifact, Some(&crux));
        assert_eq!(artifact.issues.len(), 1);
        assert!(artifact.issues[0].is_crux);
        assert_eq!(artifact.issues[0].issue, "Completely unrelated crux claim");
    }
```

- [ ] **Step 4: Run the synthesiser tests**

Run: `./scripts/sync-evo.sh check` then `ssh -i ~/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test synthesiser"`
Expected: `citation_check` tests still fail (old paths — next task); every other synthesiser test passes.

- [ ] **Step 5: Commit locally**

```bash
git add src/synthesiser/
git commit -m "feat(synthesis): SessionArtifact pipeline — fallback, salvage, crux guarantee, meta"
```

### Task 6: Retarget `citation_check.rs` to the issue paths

**Files:**
- Modify: `src/synthesiser/citation_check.rs`

- [ ] **Step 1: Update the walk.** Replace the three old field walks (`consensus_points[].evidence`, `live_disagreements[].side_{a,b}.best_argument`, `minority_positions[].key_argument`) with one nested walk:

```rust
    if let Some(issues) = synthesis.get("issues").and_then(|v| v.as_array()) {
        for (i, issue) in issues.iter().enumerate() {
            if let Some(positions) = issue.get("positions").and_then(|v| v.as_array()) {
                for (j, position) in positions.iter().enumerate() {
                    for field in ["evidence", "best_argument"] {
                        if let Some(text) = position.get(field).and_then(|v| v.as_str()) {
                            check_field_citations(
                                text,
                                &format!("issues[{i}].positions[{j}].{field}"),
                                valid_pseudonyms,
                                max_round,
                                &mut report,
                            );
                        }
                    }
                }
            }
        }
    }
```

  (Keep the existing `check_field_citations` helper and report plumbing exactly as they are — only the paths walked change. `movement[].trigger_quote` is a verbatim transcript quote, not a `[Bot, Round N]` citation, so it is deliberately NOT citation-checked.)

- [ ] **Step 2: Update the module's tests** — the two JSON fixtures at the bottom of `citation_check.rs` (~lines 233 and 260) become issue-shaped; keep the assertions' spirit (one valid-citation fixture passes, one bad-citation fixture reports):

```rust
        let synthesis = serde_json::json!({
            "topic": "t",
            "issues": [{
                "issue": "q",
                "positions": [{
                    "stance": "p",
                    "bots": ["Agent A"],
                    "best_argument": "claim [Agent A, Round 2]",
                    "evidence": "evidence [Agent A, Round 1]"
                }]
            }]
        });
```

  (and for the failure-case fixture, cite a pseudonym/round outside the valid set, mirroring the existing bad fixture's intent.)

- [ ] **Step 3: Run** — `ssh ... "cargo test citation_check"`. Expected: PASS.

- [ ] **Step 4: Commit locally**

```bash
git add src/synthesiser/citation_check.rs
git commit -m "feat(synthesis): citation check walks issue-centric paths"
```

### Task 7: Update the two integration tests

**Files:**
- Modify: `tests/five_round_flow_test.rs` (~lines 179–192)
- Modify: `tests/text_only_bot_flow.rs` (~lines 205–244)

- [ ] **Step 1: Swap the wiremock discriminator and mock bodies.** In BOTH files, the synthesis mock currently matches `body_string_contains("minority_positions")`. Change to `body_string_contains("is_crux")` (`is_crux` appears only in the new synthesis prompt — verified against divergence/extraction/crux prompts). Replace the mocked synthesis content with a populated issue (a structurally-empty mock would trip `run_synthesis`'s retry-on-empty and break `.expect(1)`):

```rust
            json!({
                "topic": "runtime preflight checks",
                "headline": "Council split on preflight value.",
                "executive_summary": "The debate considered whether preflight checks are worth the cost. Participants weighed incident-reduction claims against deployment overhead. On balance the reform case drew more support than full removal. The central unresolved issue was the correct evidentiary threshold for worth.",
                "issues": [{
                    "issue": "Whether preflight checks are worth their cost",
                    "headline": "Preflight cost-benefit threshold",
                    "is_crux": false,
                    "status": "split",
                    "positions": [{
                        "stance": "Preflight checks reduce incident volume when well-scoped.",
                        "headline": "Scoped preflight reduces incidents",
                        "bots": ["Agent A"],
                        "best_argument": "Incident data supports scoping [Agent A, Round 1]",
                        "evidence": "Agent A, Round 1 cited incident-reduction data.",
                        "final_confidence": 70,
                        "frame_rejection": false
                    }],
                    "movement": []
                }],
                "meta_observations": "Conclusion: test synthesis."
            })
```

  Update the comment blocks above each mock: the unique-marker rationale now names `is_crux` instead of `minority_positions`.

- [ ] **Step 2: Grep for stragglers**

Run: `grep -rn "consensus_points\|minority_positions\|live_disagreements\|flagged_capitulations\|confidence_trajectories" src/ tests/`
Expected: zero hits (the frontend keeps its old references until Phase 2 — `frontend/` is out of scope).

- [ ] **Step 3: Full test suite on EVO**

Run: `./scripts/sync-evo.sh`
Expected: `cargo test --all` PASS.

- [ ] **Step 4: fmt + clippy**

Run: `ssh -i ~/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo fmt --check && cargo clippy --all-targets"`
Expected: fmt clean (run `cargo fmt` locally-on-EVO and re-scp if not); clippy no new warnings.

- [ ] **Step 5: Commit and push everything**

```bash
git add tests/
git commit -m "test: integration mocks emit SessionArtifact shape, is_crux discriminator"
git push -u origin claude/issue-centric-schema
```

### Task 8: PR

- [ ] **Step 1: Open the PR**

```bash
gh pr create --base main --head claude/issue-centric-schema --title "feat(synthesis): issue-centric SessionArtifact schema (Phase 1)" --body "$(cat <<'EOF'
## Summary
- Replaces the five-list SynthesisOutput with the issue-centric SessionArtifact (spec: docs/superpowers/specs/2026-07-02-issue-centric-sessions-design.md, Part 1)
- Rewrites the synthesis prompt to emit issues/positions/movement; crux now reaches the artifact (ensure_crux_issue guarantee)
- Splits the 1759-line synthesiser mod.rs into client/evidence/prompt/meta modules (300-line rule)
- Retargets citation_check to issue paths; keeps all salvage/fallback/serde-default safety nets

## NOT shipped alone
Deployment is gated on the Phase 2 frontend PR — the live UI reads the old payload shape. One ship.sh after Phase 2 merges, then resynthesise --all on EVO (lesson 16).

## Test plan
- [x] cargo test --all on EVO (schema fixtures, prompt contract, crux guarantee, meta composition, citation paths, integration flows)
- [x] cargo fmt --check && cargo clippy --all-targets
- [ ] ship.sh + resynth deferred to Phase 2 (see above)
🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Wait for CI green, then squash-merge with `--delete-branch`.** Do NOT run `ship.sh`.

---

## Self-review notes (already applied)

- Spec coverage: Part 1 schema (Task 1), semantics mapping incl. movement/justified and frame_rejection (Tasks 1, 4, 5), crux guarantee "always present when a crux_selection row exists" (Task 5 `ensure_crux_issue` + tests), supersede-not-wrap with legacy-parse test (Task 1), prompt rewrite with retained headline/exec-summary/abstention rules (Task 4), error-handling rows "MiniMax drops fields" and "no issues at all" (serde defaults + structural fallback, Tasks 1/5), testing section's schema/crux/mapping/prompt items (Tasks 1–7). Resynth compatibility: `resynth.rs` calls `run_synthesis` and stores the string — no change needed there; historical-transcript resynth is exercised operationally after Phase 2 ship.
- `headline` on the artifact and `Issue`/`Position` names/signatures are consistent across Tasks 1, 4, 5, 6, 7.
- The wiremock discriminator change is the easiest thing to miss; it is called out in working notes AND Task 7.
