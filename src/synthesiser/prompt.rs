//! Synthesis prompt construction (issue-centric SessionArtifact output).

use crate::analyser::crux::CruxSelection;
use crate::sanitise::ANTI_INJECTION_PREAMBLE;
use crate::synthesiser::evidence::parse_grounding_evidence;
use std::collections::{BTreeMap, BTreeSet};

/// Build an at-a-glance abstention summary derived from grounding_evidence
/// so the synthesiser sees which bots gapped which rounds without having
/// to scan a 40-kB grounding array. `effective_abstained` was written by
/// the LLM classifier at synthesis prep; `abstained` is a formal opt-out.
///
/// For each abstaining bot, also captures a short verbatim quote of their
/// first non-substantive response — this is the actionable signal the bot
/// operator needs to diagnose the failure (e.g. wrapper-emitted "could
/// not complete the upstream model call"). The synthesis prompt
/// instructs the model to surface this verbatim under
/// `meta_observations` "Bot behaviour notes" so the operator sees it.
fn derive_abstention_summary(grounding_evidence_json: &str) -> String {
    let entries = parse_grounding_evidence(grounding_evidence_json);
    if entries.is_empty() {
        return "No grounding evidence available.".into();
    }

    let mut all_agents: BTreeSet<String> = BTreeSet::new();
    let mut gap_rounds: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    let mut first_gap_quote: BTreeMap<String, (i64, String)> = BTreeMap::new();
    for e in &entries {
        if e.agent.trim().is_empty() {
            continue;
        }
        all_agents.insert(e.agent.clone());
        if e.abstained || e.effective_abstained {
            gap_rounds.entry(e.agent.clone()).or_default().push(e.round);
            let quote: String = e
                .response
                .trim()
                .replace('\n', " ")
                .chars()
                .take(240)
                .collect();
            first_gap_quote
                .entry(e.agent.clone())
                .and_modify(|existing| {
                    if e.round < existing.0 {
                        *existing = (e.round, quote.clone());
                    }
                })
                .or_insert((e.round, quote));
        }
    }

    if all_agents.is_empty() {
        return "No grounding evidence available.".into();
    }
    if gap_rounds.is_empty() {
        return "All participating bots engaged substantively in every round.".into();
    }

    let mut lines = Vec::new();
    for agent in &all_agents {
        match gap_rounds.get(agent) {
            Some(rounds) => {
                let mut rs = rounds.clone();
                rs.sort();
                rs.dedup();
                let rounds_str = rs
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let quote = first_gap_quote
                    .get(agent)
                    .map(|(_, q)| q.as_str())
                    .unwrap_or("");
                if quote.is_empty() {
                    lines.push(format!(
                        "{agent}: effectively abstained in round(s) {rounds_str} — treat these rounds as silence, not as substantive contributions."
                    ));
                } else {
                    lines.push(format!(
                        "{agent}: effectively abstained in round(s) {rounds_str}. Self-reported signal (verbatim, for the bot operator to diagnose): \"{quote}\""
                    ));
                }
            }
            None => {
                lines.push(format!("{agent}: engaged in every round."));
            }
        }
    }
    lines.join("\n")
}

/// Build the full synthesis prompt from debate artifacts.
///
/// When `crux` is `Some`, a dedicated "Crux outcome" section is inserted
/// requiring the model to emit exactly one issue with `is_crux: true`,
/// its status derived from the crux's outcome. When `crux` is `None`
/// (crux selection failed or the debate pre-dates crux selection) the
/// section is omitted entirely — no empty header.
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

#[cfg(test)]
mod tests {
    use super::build_synthesis_prompt;
    use crate::analyser::crux::CruxSelection;

    #[test]
    fn prompt_contains_issue_schema_and_contract() {
        let p = build_synthesis_prompt(
            "topic",
            "Agent A = Alice",
            "transcript",
            "{}",
            "[]",
            "[]",
            None,
        );
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
        let p = build_synthesis_prompt(
            "topic",
            "Agent A = Alice",
            "transcript",
            "{}",
            "[]",
            "[]",
            None,
        );
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
        let p = build_synthesis_prompt(
            "topic",
            "Agent A = Alice",
            "transcript",
            "{}",
            "[]",
            "[]",
            Some(&crux),
        );
        assert!(p.contains("Crux outcome"));
        assert!(p.contains("SOC 2 costs are trivial"));
        assert!(p.contains("exactly one issue"));
        assert!(p.contains("crux_shift"));
    }

    #[test]
    fn crux_section_omitted_when_absent() {
        let p = build_synthesis_prompt(
            "topic",
            "Agent A = Alice",
            "transcript",
            "{}",
            "[]",
            "[]",
            None,
        );
        assert!(!p.contains("Crux outcome"));
    }
}
