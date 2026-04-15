# Bot Council — Continuation Instructions for New Agent

> Read this file, then read the files listed below in order. Do not start work until you have read all of them.

## What This Project Is

The LQ Bot Council is a Rust/Axum service that orchestrates structured adversarial debates between AI bots. Bots register via HTTP, receive debate topics, submit positions, and (in later phases) engage in multi-round adversarial debate with anti-sycophancy enforcement.

## Current State: Phase 0 Complete

Phase 0 (single-shot MVP) is fully implemented and tested on the Evo X2. The harness can:
- Register bots (`POST /bots`)
- Create single-shot debates (`POST /debates`)
- Dispatch topics to all bots concurrently
- Anonymise responses (Agent A, B, C...)
- Collect peer scores (bots score each other, can't score themselves)
- Aggregate into ranked results
- Persist everything in SQLite

All 6 integration tests pass. End-to-end smoke test verified with 3 reference bots on the Evo X2.

## Files You Must Read (in order)

1. **`CLAUDE.md`** — Coding standards, deploy workflow, quick reference. BINDING.
2. **`docs/superpowers/specs/2026-04-15-bot-council-harness-design.md`** — Full design spec (v1.2). Covers all phases, the protocol, anti-sycophancy mechanisms, judge model, reputation, LQ Brain, known limitations, and the phased build plan.
3. **`docs/superpowers/plans/2026-04-15-phase0-single-shot-mvp.md`** — Phase 0 implementation plan (completed). Read to understand how the codebase was built and the patterns used.

## Architecture Summary

```
bot-council/
  src/
    main.rs              — tokio entry, builds app, listens on port 3100
    lib.rs               — module declarations, build_app()
    config.rs            — Settings loaded from config/default.toml + APP__* env vars
    error.rs             — AppError enum with IntoResponse (thiserror)
    types.rs             — DebateId, BotId newtypes, DebateStatus enum
    state.rs             — AppState (Arc<Inner> with db, http_client, settings)
    api/
      mod.rs             — Router assembly
      auth.rs            — BearerAuth extractor
      bots.rs            — POST/GET /bots
      debates.rs         — POST/GET /debates, GET /debates/{id}
      dto.rs             — All request/response types
      health.rs          — GET /health
    orchestrator/
      mod.rs             — run_debate() — dispatches, anonymises, scores, aggregates
      anonymiser.rs      — assign_pseudonym()
    bot_client/
      mod.rs             — HTTP client with retry, send_position_request, send_scoring_request
    db/
      mod.rs             — init_pool with WAL pragmas + migrations
      models.rs          — Row structs (BotRow, DebateRow, etc.)
      queries.rs         — All SQL query functions
  migrations/
    20260415000001_init.sql  — bots, debates, debate_bots, responses, peer_scores
  tests/
    common/mod.rs        — test_app() helper with in-memory SQLite
    api_bots_test.rs     — 2 tests
    api_debates_test.rs  — 3 tests
    api_health_test.rs   — 1 test
  reference/
    debate-endpoint-node.js    — Reference bot endpoint (Node.js)
    debate-endpoint-python.py  — Reference bot endpoint (Python)
    run-smoke-test.sh          — E2E smoke test script
  config/
    default.toml         — Default config (port 3100, SQLite path, timeouts)
```

## Build and Test

The project builds on the Evo X2, NOT locally (C: drive is too small for Rust debug builds).

```bash
# Sync local changes to Evo
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests config migrations Cargo.toml Cargo.lock james@100.90.66.54:~/bot-council/

# Build and test on Evo
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"

# Run the harness
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo run"
```

Evo X2 SSH: `james@100.90.66.54` (Tailscale), key at `C:\Users\James\.ssh\id_ed25519`.

## What to Build Next: Phase 1

Phase 1 adds the full 5-round adversarial protocol on top of the Phase 0 infrastructure. Read the "Phase 1 — Multi-Round Protocol" section of the design spec carefully.

Key additions:
- **Constitutional roles** (Proponent, Skeptic, Devil's Advocate, Empiricist, Steelman) with rotation tracking
- **Rounds 0-4** as a state machine with resumption
- **Anonymisation across rounds** (not just single-shot)
- **Structured challenge field** in bot responses + **MiniMax validation** (Round 2 dissent gate)
- **Cross-examination pairing** via MiniMax (Round 3 — two sub-passes)
- **Position change tracking** (Round 4)
- **Divergence analysis** via MiniMax (post-Round 4)
- **Opus synthesis** with rigid JSON schema, temperature 0, citation-required
- **Full transcript API** with anonymisation log

### Architectural Decisions Already Made

- MiniMax M2.7 for all harness-internal analytical calls (challenge validation, divergence scoring, cross-exam pairing). Cheap, adequate for structured comparison.
- Claude Opus 4.6 for synthesis only. Temperature 0, rigid schema, every claim must cite [pseudonym, Round N].
- No local embedding models. Divergence detection is an LLM reasoning task, not a vector similarity task.
- Bots bring their own context — no session isolation. The protocol handles strong priors structurally.
- The Synthesist role was dropped from bot roles — it's harness-only (the Opus synthesis pass). Five bot roles: Proponent, Skeptic, Devil's Advocate, Empiricist, Steelman.

### Community Feedback to Incorporate

- Artur: "perhaps we start with something super simple... brick by brick" — Phase 0 did this. Phase 1 should also be built incrementally (e.g., roles first, then multi-round, then MiniMax validation, then synthesis).
- LQ_Alice: MiniMax as sole judge is single point of drift — acknowledged as known limitation, ensemble judging deferred to Phase 3.
- LQ_Alice: "keep: blind R0, anonymisation, structured challenge gate with MiniMax validation, position-change detection, state machine + resumption, Opus synthesis. That's ~60% of the spec and where the interesting work is." — This is exactly Phase 1.
- Artur: roles are prompt-based which tensions with "structural not prompting" principle — documented in Known Limitations. The enforcement mechanisms (re-prompting) add structural backing.

### Key Design Constraints

- The `/debate` bot API contract must remain backward-compatible with Phase 0. New fields (role, challenge, position_change) are added but old fields still work.
- The `round` field changes from `0 | "scoring"` (Phase 0) to `0-4` (Phase 1). The scoring round from Phase 0 is replaced by the judge model in Phase 2.
- The state machine must be resumable — every transition persisted to SQLite. If the harness crashes mid-debate, it picks up from the last completed step.
- The orchestrator's `run_debate()` function will need significant expansion or replacement. Consider splitting into per-round functions.

### Suggested Build Order for Phase 1

1. Add roles module (definitions, rotation tracking, assignment)
2. Expand the state machine (Rounds 0-4 transitions in SQLite)
3. Implement Round 0 (blind formation — similar to Phase 0 dispatch but with role assignment)
4. Implement Round 1 (anonymised redistribution + identify strongest opposing argument)
5. Implement Round 2 (structured rebuttal + MiniMax challenge validation + re-prompt loop)
6. Implement Round 3 (MiniMax pairing + cross-examination two-pass)
7. Implement Round 4 (final position + position_change declaration)
8. Implement divergence analysis (MiniMax per-bot comparison)
9. Implement Opus synthesis (pre-computation + synthesis call)
10. Update API endpoints (transcript, synthesis retrieval)
11. Update reference bot endpoints to handle all rounds
12. E2E test with reference bots

Write a proper implementation plan (use superpowers:writing-plans) before starting.
