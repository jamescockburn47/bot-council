# Five-round debate redesign — design

**Date:** 2026-04-22
**Author:** James Cockburn (with Claude)
**Status:** Design — awaiting implementation plan
**Depends on:** [text-only bot mode](./2026-04-22-text-only-bot-mode-design.md) (shipped Phase 1 + 2)
**Ships with:** Bot guide "Getting a public URL" section (already written, bundled in this PR)

## Problem

A recent debate export ([bcf11f75](https://lqcouncil.com/debates/bcf11f75-f85d-4391-b634-f2f2e180d82f)) exposed four compounding weaknesses:

1. **Short-circuit to 3 rounds.** The `APP__DEBATE__TEST_MODE_SIMPLE=true` environment override in `/etc/bot-council.env` shortcut the protocol to rounds 0–2 only, skipping the designed cross-examination (R3) and final-position (R4) rounds. The 5-round code in [src/orchestrator/multi_round.rs](../../../src/orchestrator/multi_round.rs) was already built but not exercised.
2. **Half the council silent for half the debate.** Oscar abstained in R1 and R2 (orchestrator-side failure: HTTP/timeout/decode — see [src/orchestrator/rounds/round1.rs:75-78](../../../src/orchestrator/rounds/round1.rs:75)); Jamie-LQClaw returned the stock phrase "I was unable to formulate a response" in R1 and R2 (bot-side format rejection). Two of four participants contributed one round each out of three.
3. **Thin prompts.** The current round prompts in [src/orchestrator/prompts.rs](../../../src/orchestrator/prompts.rs) don't ask for sources, minimum depth, engagement with named peers, or steelman reasoning. Only one of four bots (Clint) cited sources voluntarily.
4. **Weak role rotation in perceived practice.** Current rotation at [src/orchestrator/roles.rs:34-43](../../../src/orchestrator/roles.rs:34) avoids *consecutive* same-role only. Over short debate histories this doesn't feel like rotation — a bot can cluster on one role across most recent debates.

The cumulative effect: debates that truncate before the planned adversarial rounds, with half the voices missing, producing generic arguments that don't engage the specific crux of disagreement.

## Goals

- Restore the full 5-round protocol as the only mode.
- Maximise genuine, unusual insights by:
  - Demanding sources, depth, and named-peer engagement in prompts.
  - Forcing engagement with the debate's actual crux in R3 instead of drifting cross-examination Q&A.
  - Adding a steelman requirement before the final round.
- Fix the abstention cascade: one transient bot failure shouldn't silence that bot for the rest of the debate.
- Simplify role rotation to pure randomisation — current complexity isn't paying for itself.
- Document how bot owners get a public URL (stops the Jamie-LQClaw quick-tunnel failure mode at source).

## Non-goals

- Text-only bot mode was landed separately ([spec](./2026-04-22-text-only-bot-mode-design.md)) and isn't re-opened here.
- Synthesis prompt rewrites — the existing synthesis prompt already handles the structures we need, though it gains one new section (crux outcome). No larger synthesis overhaul.
- UI redesign beyond the targeted additions called out below. Existing debate-detail and bots pages keep their layout.
- Backwards compatibility with historical debates. Resynth CLI continues to work; new fields are optional.

## Approach

One PR, one migration, one shippable unit. The change surface:

- **Protocol:** remove `test_mode_simple` entirely; kill its token-check bypass in preflight.
- **Prompts:** rewrite all five round prompts plus add a crux-aware R3 prompt and a steelman-requiring R4 prompt.
- **Rounds code:** retry-with-simpler-prompt then R0-carry-forward on bot failure; new crux selector between R1 and R3.
- **Roles:** collapse to pure shuffle.
- **Extraction:** add `steelman` (R4) and `crux_engagement` (R3) field extraction using the existing MiniMax-with-source-quote pipeline.
- **Data model:** two new nullable columns on `responses`, plus a new `crux_selection` analysis kind. No new tables. No `natural_role` column.
- **UI:** transcript shows the crux claim at R3, shows the steelman extraction at R4, and badges R0-carry-forward responses.
- **Docs:** bot guide gains a "Getting a public URL" section (already written).

## Components

### Files touched

| File | Change |
|---|---|
| [config/default.toml](../../../config/default.toml) | Remove `test_mode_simple` entirely. |
| [src/config.rs](../../../src/config.rs) | Drop the `test_mode_simple` field from `DebateConfig`. |
| [src/api/debates.rs:72](../../../src/api/debates.rs:72) | Token-null preflight check always enforced — no `simple_mode` gate. |
| [src/api/bots.rs](../../../src/api/bots.rs) | Remove the `simple_mode` branches (token optionality, auto-approve path). |
| [src/orchestrator/multi_round.rs](../../../src/orchestrator/multi_round.rs) | Remove `simple_mode` branches (R2-as-final short-circuit, round name renaming). 5 rounds always. |
| [src/orchestrator/prompts.rs](../../../src/orchestrator/prompts.rs) | Rewrite all five round prompts. Add R3 crux prompt + R4 steelman extension. Remove `round2_prompt_simple`. |
| [src/orchestrator/rounds/round1.rs](../../../src/orchestrator/rounds/round1.rs) | Retry-on-failure + R0-carry-forward path. |
| [src/orchestrator/rounds/round2.rs](../../../src/orchestrator/rounds/round2.rs) | Same resilience pattern. |
| [src/orchestrator/rounds/round3.rs](../../../src/orchestrator/rounds/round3.rs) | Replace cross-examination (question/answer pairing) with crux-round dispatch. |
| [src/orchestrator/rounds/round4.rs](../../../src/orchestrator/rounds/round4.rs) | Same resilience pattern + feed steelman extraction. |
| [src/orchestrator/roles.rs](../../../src/orchestrator/roles.rs) | Collapse to pure shuffle. No DB reads. Keep `persist_role_assignments` for audit. |
| [src/orchestrator/extraction.rs](../../../src/orchestrator/extraction.rs) | Add `steelman` (R4) and `crux_engagement` (R3) extraction with source-quote verification. |
| [src/analyser/crux.rs](../../../src/analyser/) (new) | Crux selector. |
| [src/analyser/divergence.rs](../../../src/analyser/divergence.rs) | Add `crux_shift` to per-bot divergence output. |
| [src/synthesiser/mod.rs](../../../src/synthesiser/mod.rs) | Synthesis prompt gains a "crux outcome" section referencing `crux_shift`. |
| `migrations/20260423000001_crux_and_resilience.sql` (new) | Two `responses` columns. |
| [frontend/src/routes/debates/[id]/+page.svelte](../../../frontend/src/routes/debates/[id]/+page.svelte) | Render crux claim at R3 header, steelman field at R4, carry-forward badge on fallback responses. |
| [frontend/src/routes/bots/guide/+page.svelte](../../../frontend/src/routes/bots/guide/+page.svelte) + [snippets.ts](../../../frontend/src/routes/bots/guide/snippets.ts) | Already done — public URL recipes. |
| `tests/` | Extensions listed in Testing below. |

## Round-by-round protocol

| Round | Name | Prompt demands (new) | Extraction |
|---|---|---|---|
| **R0** | Blind Formation | Cite ≥3 sources inline. Minimum 500 words. State position without hedging. Role description injected as today. | None. |
| **R1** | Anonymous Distribution | Identify single strongest opposing argument: name the pseudonym, quote verbatim, provide counter-evidence citing ≥1 source not used in R0. | None. |
| **R2** | Structured Rebuttal | As today: challenge (`claim_targeted`, `counter_evidence`, `type`) with source. Rejection-retry path if challenge is missing. | `challenge` (existing). |
| **R3** | Crux Engagement (new) | Analyser selects the single most divergent R1 claim. Every bot receives: *"The debate's central disagreement is X, stated by Y: 'verbatim quote'. Engage it directly. Hold what you can defend; concede only what you cannot. Capitulation without specific new evidence will be flagged. If you reject the framing of this crux itself — false dichotomy, missing variable, wrong level of abstraction — state that and what the right framing would be. Do not engage on a frame you believe to be broken. Frame-rejection without justification will also be flagged."* | `crux_engagement` (new) via MiniMax extraction with quote verification. |
| **R4** | Final Position | As today (final position, `position_change` with `from_summary`/`to_summary`/`reason`), plus new requirement: before the final position, articulate the strongest version of the opposing argument in 2–3 sentences (the steelman). Plus: *"If you still hold disagreements beyond the R3 crux, state them — the crux is the debate's centre of mass, not its only point."* | `position_change` (existing) + `steelman` (new), both source-quote verified. |

### Anti-sycophancy coda

Every R1–R4 prompt ends with:

> *"Maintain your position unless the evidence compels otherwise. Capitulation without named new evidence will be flagged in synthesis. Novel insight is valued above agreement."*

## Role assignment

Replace the full contents of [src/orchestrator/roles.rs::assign_roles](../../../src/orchestrator/roles.rs) with a pure shuffle:

```rust
pub async fn assign_roles(
    _pool: &SqlitePool,
    bot_ids: &[String],
) -> Result<Vec<(String, Role)>, String> {
    if bot_ids.len() > 5 {
        return Err("maximum 5 bots per debate".into());
    }
    let mut roles: Vec<Role> = Role::ALL[..bot_ids.len()].to_vec();
    roles.shuffle(&mut rand::rng());
    Ok(bot_ids.iter().cloned().zip(roles.into_iter()).collect())
}
```

No DB read. No consecutive-guard. No natural-role preference. Long-run distribution is uniform; short-run clustering is accepted — the cost of preventing it outweighs the benefit at the debate cadence we run.

`persist_role_assignments` remains unchanged — it continues to populate `role_history` for audit/UI, but that table is no longer consulted during assignment.

## Abstention resilience

Three-layer fallback on R1+ dispatch failure. Applies whenever the bot returns:

- HTTP error (non-2xx).
- Timeout.
- A response whose `response` text matches `is_effective_abstention_response` (the `"unable to formulate"`, `"cannot provide"` family of stock refusals).
- A response that fails structural validation for that round (e.g., missing `challenge` on R2).

### Layer 1 — Retry with simplified prompt

One retry, using the same per-round timeout budget (`debate.default_timeout_secs`, 300s by default). Retry prompt:

> *"Answer this round in one paragraph using your prior-round position as a starting point. If you genuinely cannot, reply with one sentence explaining why. Topic: {topic}. Current round: {N}."*

Tracked as `responses.retry_count` (new column, default 0). The retry's result is stored; the initial failure isn't persisted.

### Layer 2 — R0 carry-forward

If the retry also fails (any of the above triggers), write a response row with:

- `response_json = <bot's R0 response>` (read from DB).
- `fallback_from_round = 0` (new column).
- `abstained = false`, `valid = true`, `retry_count = 1`.

The bot stays in-debate at its R0 position. Subsequent rounds treat this as a real response (so other bots see it when distributing anonymised context). Synthesis input (`grounding_evidence_json`) tags carry-forwards explicitly so the synthesiser can note "Agent X held their R0 position through rounds N–M" if material.

### Layer 3 — True abstention

If R0 itself failed (no prior-round content to carry), the bot is marked abstained for the current round as today. This path matches current behaviour.

### Transcript UI

Fallback responses render with a `carried from R0` badge and a muted tone. Carry-forward is visible but not punitive — the reader knows the bot didn't actively re-engage, without the voice being absent.

## Crux selection

### Pipeline position

Runs sequentially between R2 completion and R3 dispatch. The selector's only inputs are R0 and R1 text (R2 rebuttals aren't needed — R1 already contains each bot's named strongest counter, which is the richest divergence signal), so running it post-R2 is a simplicity choice rather than a correctness requirement. A future optimisation could run it concurrently with R2 dispatch to save ~30s wall-clock, but that's out of scope here.

### Algorithm

New file [src/analyser/crux.rs](../../../src/analyser/crux.rs). Single MiniMax call with strict JSON schema:

**Input** assembled by the orchestrator:

```json
{
  "topic": "<debate topic>",
  "r1_responses": [
    {"pseudonym": "Agent A", "r0": "<r0 text>", "r1": "<r1 text>"},
    ...
  ]
}
```

**Prompt:**

> *"{N} participants wrote R0 and R1 responses. The R1 responses each identified the strongest opposing argument. Identify the single claim across these responses that creates the widest, sharpest disagreement — the one where participants most visibly clash.*
>
> *Return exactly:*
>
> ```
> {"claim": "<1-sentence restatement of the claim>",
>  "source_pseudonym": "<who first stated it>",
>  "source_quote": "<verbatim substring>"}
> ```
>
> *No prose outside the JSON."*

### Verification

`source_quote` must pass `extractor::verify::quote_is_substring_of` against the source pseudonym's R1 text. If the quote fails verification:

- Re-prompt once with the failure reason ("the quote was not a substring; try again with exact text").
- If that also fails, fall back to today's cross-examination format (round_name R3 = "Cross-Examination", bots paired as before). Log a warning. The crux feature degrades gracefully; no debate is blocked.

### Persistence

Inserted into the existing `analyses` table with `kind='crux_selection'`. `input` is the R1 response collection (serialised). `result` is the verified `{claim, source_pseudonym, source_quote}` JSON.

### crux_shift in divergence analysis

[src/analyser/divergence.rs](../../../src/analyser/divergence.rs) gains a field: for each bot, compare their R1 stance on the crux claim to their R3 engagement. Classify as `resolved_toward_crux`, `resolved_against_crux`, `unchanged`, `frame_rejected`, or `no_engagement`. This per-bot classification feeds synthesis so convergence vs. hardening is visible in the output, not hidden.

## Data model

Single new migration `migrations/20260423000001_crux_and_resilience.sql`:

```sql
-- Retry count for abstention resilience. 0 = first-attempt success or true abstention.
ALTER TABLE responses ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;

-- If non-NULL, this response is a carry-forward from the named earlier round
-- (only 0 is used today; schema allows future variants).
ALTER TABLE responses ADD COLUMN fallback_from_round INTEGER NULL;
```

No new tables. No changes to `bots`, `debates`, `analyses`, or `role_history`. No `natural_role` column.

Existing `analyses.kind` column already accepts arbitrary strings — `crux_selection` hangs off it without schema change.

## Error handling

| Failure | Response |
|---|---|
| Bot HTTP error on R0 | Abstained for R0 (as today). Bot loses all subsequent rounds (carry-forward needs R0 content). |
| Bot HTTP error on R1–R4 | Retry with simplified prompt → R0 carry-forward → (only if R0 also empty) abstain. |
| Bot returns "unable to formulate" text | Treated as failure; same retry → carry-forward ladder. |
| Bot response fails structural validation (e.g. R2 missing challenge) | One rejection-reprompt (existing behaviour) → retry-simplified on that → carry-forward. |
| Crux selector: MiniMax returns malformed JSON | One retry with same prompt. |
| Crux selector: source_quote fails substring verification | One retry with failure-reason appended. |
| Crux selector: both retries fail | R3 reverts to legacy cross-examination format. Warning logged with `debate_id`. Synthesis sees no `crux_selection` analysis row; synthesiser prompt tolerates the absent section. |
| Steelman extraction fails verification | `steelman` field omitted from R4 response. Synthesis tolerates absent steelman. |
| Two bots fail preflight, one survives | Quorum (≥3) not met. Debate creation returns 400 today; that stays. |

## Testing

### Unit

- `roles::assign_roles`: property test — 1,000 shuffles over 3–5 bots return uniformly-distributed role assignments within statistical tolerance. Error path: >5 bots rejected.
- `prompts::*`: snapshot tests on rewritten prompts covering source-count instructions, steelman requirement, frame-rejection permission, anti-sycophancy coda. Tests fail deterministically if a prompt drifts.
- `analyser::crux::select`: mocked MiniMax client. Covers: happy path; malformed JSON → one retry; invalid source_quote → one retry then fallback; network failure → fallback.
- `extraction::steelman` + `extraction::crux_engagement`: source-quote verification passes on substring match, downgrades to `source: "extraction_failed"` on mismatch (same pattern as existing `challenge` extraction).
- `multi_round::retry_abstaining_bot`: simulated bot that fails first call, succeeds on retry. Verify `retry_count=1` persisted.
- `multi_round::carry_forward_r0`: simulated bot that fails twice on R2. Verify response row written with `fallback_from_round=0` and R0 text.

### Integration (`tower::ServiceExt::oneshot` against in-memory sqlite)

- `tests/multi_round_debate_test.rs` (existing, extended): full 5-round debate; at least one bot triggers the retry path; one bot triggers carry-forward; verify synthesis receives carry-forward tags.
- `tests/crux_round_test.rs` (new): synthetic divergence in R1 → crux selected → all bots dispatched with crux-inclusive prompt → `crux_engagement` extracted.
- `tests/api_debates_test.rs` (existing, extended): POST `/api/debates` with 5 bots runs the full 5-round flow ending in synthesis with all new fields.
- `tests/text_only_bot_flow.rs` (existing): re-verify text-only path still works end-to-end with new prompts.

### Resynth CLI compatibility

`bot-council resynthesise <old_debate_id>` must still work for debates that pre-date the crux round. Synthesis prompt tolerates missing `crux_selection` and missing `steelman`/`crux_engagement` fields.

### Migration

Fresh DB + migrate → schema has new columns.

Existing DB + migrate → two columns added, all existing rows get defaults (`retry_count=0`, `fallback_from_round=NULL`). No data loss.

Rollback: `sqlx migrate revert` drops the two columns. Not automated — this is a forward-only change in practice.

### CI

`.github/workflows/ci.yml` already runs `cargo fmt --check`, `cargo clippy`, `cargo test --all` (backend) and `npm ci && npm run build` (frontend). No CI changes needed. Both jobs must pass before merge.

## Migration / rollout

1. Merge PR. Migration runs automatically on `bot-council` startup via `sqlx::migrate!`.
2. **Critical env change:** remove `APP__DEBATE__TEST_MODE_SIMPLE` from `/etc/bot-council.env` on EVO (the field no longer exists on `DebateConfig`; leaving it in the env file is harmless but misleading). Do this before the first post-merge `ship.sh` or the service fails to start due to unknown-field rejection (depending on serde config).
3. `./scripts/ship.sh` — standard ship workflow.
4. Run `bot-council resynthesise --all --throttle-ms 2000` on EVO after the new synthesis prompt change is verified on one manual debate. (Synthesis gained a crux-outcome section; historical debates need resynth to benefit from refined language — but they're still valid without it.)
5. Spot-check: create one admin-smoke debate with the text-only schema, verify all 5 rounds fire, transcript shows crux in R3 and steelman in R4.

## Risks and open questions

### Risk: crux round narrows the debate to one axis

Three other live disagreements could lose momentum while everyone focuses on the crux. Mitigations: R4 prompt explicitly re-opens non-crux disagreements. Synthesiser still emits `live_disagreements` unchanged. If this is observed repeatedly, a follow-up work can make `crux_count` configurable (default 1, experimental 2) without a second migration.

### Risk: abstention carry-forward dilutes debate quality

A bot that actually has nothing new to say gets its R0 position repeated. This is better than silence (synthesis has something to work with) but worse than an active R3 response. Mitigation: UI badge makes carry-forward visible to readers. Synthesis prompt gains a line distinguishing "held position through" from "actively defended". Operators see the pattern in the transcript and can deactivate low-signal bots.

### Risk: source-count requirement produces fabricated citations

Asking for ≥3 sources in R0 may push weaker bots toward citation fabrication. Mitigation: the R0 prompt explicitly says *"cite only sources you can quote verbatim; invented citations will fail human review and flag the bot for re-approval"*. Detection of fabricated citations isn't in scope for this PR but is tracked as follow-up work (candidate: a post-synthesis citation-grounding check against web search).

### Risk: text-only bots produce prose that won't extract cleanly

The existing text-only extraction (R2 `challenge`, R4 `position_change`) already handles variable prose well. `steelman` and `crux_engagement` use the same pipeline. The R3 prompt is structured enough that extraction should succeed on any coherent response; prose that rejects the frame will extract `crux_engagement.frame_rejected=true` rather than failing silently.

### Open question: synthesis weight for carry-forwards

Currently the synthesiser sees all non-abstained responses with equal footing. Should carry-forwards be down-weighted (e.g. excluded from consensus counts)? Initial answer: no — they represent a bot's actual position, not noise. Revisit after 10+ live debates if consensus counts look inflated.

### Open question: retain `role_history` writes?

Since `role_history` is no longer read by `assign_roles`, it's technically redundant. Initial answer: yes, retain writes — the admin UI displays per-bot role history on the analytics page, and the write cost is trivial. If the admin UI drops that feature, `role_history` can be retired in a follow-up.

## Appendix A — public URL guide (already written)

Shipped in this PR as [frontend/src/routes/bots/guide/+page.svelte](../../../frontend/src/routes/bots/guide/+page.svelte) + [snippets.ts](../../../frontend/src/routes/bots/guide/snippets.ts). Documents three recipes: Cloudflare Workers (serverless, recommended for new bots), DuckDNS + Caddy (recommended for bots running on a VPS or other server), and own-domain + Caddy. Explicit warning against Cloudflare quick tunnels.

## Appendix B — what this design deliberately excludes

- Automatic bot-side conversion of Jamie-LQClaw / Oscar to `text_only` mode (operator-side action; not a code change).
- Fleet diagnostics UI (Clint/Oscar NULL-token detection, Alice/LQClaw 502 detection). Worth doing, not in this PR.
- Synthesis prompt overhaul beyond the crux-outcome section.
- Changes to the peer-scoring round — it still runs as today against R4 responses only.
- CI additions — current backend+frontend jobs cover the new tests.
