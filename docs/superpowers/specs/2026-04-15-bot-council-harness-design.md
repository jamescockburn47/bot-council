# LQ Bot Council Harness — Design Specification

> v1 — 2026-04-15 — James Cockburn

## Overview

A standalone Rust/Axum service that orchestrates structured multi-agent adversarial debates. The harness manages a 5-round protocol with anti-sycophancy mechanisms enforced structurally, not through prompting. It communicates with N participating bots via HTTP, persists full session state in SQLite, and produces a rigorous synthesis output via a tightly-prompted Opus call.

The harness is general-purpose. It is not coupled to any specific bot implementation, WhatsApp, or the Clawdbot codebase. It runs on the Evo X2 (AMD Strix Halo, 128GB UMA) alongside existing services but has no dependency on them.

## Design Principles

1. **Structural enforcement over prompting.** Anti-sycophancy mechanisms are protocol rules enforced by the harness, not instructions in bot prompts that can be ignored.
2. **Bot-agnostic.** The harness does not know or care what model, memory system, or tool stack any bot uses. It calls a uniform HTTP contract.
3. **Resumable.** Every state transition is persisted. If the harness crashes mid-debate, it resumes from the last completed step.
4. **Auditable.** Full round-by-round transcript with anonymisation log, divergence analysis, and synthesis provenance. Every claim in the synthesis cites bot + round.
5. **Minimal dependencies.** Pure HTTP orchestration + SQLite + cloud LLM calls (MiniMax for analysis, Opus for synthesis). No local embedding models, no message queues, no external databases.

## Participants

Each debate involves up to N bots (default 5 for the LQ community). Each bot is registered with:

- Unique ID
- Display name
- Endpoint URL (`POST /debate`)
- Bearer token (outbound, harness → bot)
- Active flag

Bots bring their own context, tools, and knowledge. There is no session isolation requirement — a bot's accumulated knowledge enriches the debate. The protocol's blind formation and adversarial rounds handle the case where a bot arrives with strong priors.

## Constitutional Roles

Five roles, one per bot, rotated across debates:

| Role | Function | Enforcement |
|------|----------|-------------|
| **Proponent** | Constructs the strongest case for the proposition | Harness flags responses that concede the core proposition without challenge |
| **Skeptic** | Challenges assumptions and demands evidence | Must include at least one explicit doubt or evidence request per round |
| **Devil's Advocate** | Argues positions it may not hold to stress-test reasoning | Harness tracks whether DA maintains contrarian posture; premature agreement triggers re-prompt |
| **Empiricist** | Demands factual grounding, flags unsupported assertions | Must identify at least one unsupported claim per round from Round 1 onward |
| **Steelman** | Strengthens opposing arguments before engaging them | Must articulate the strongest version of an opposing position before critiquing it |

Role assignment is random per debate. The harness tracks assignment history across debates to ensure balanced rotation (no bot gets the same role in consecutive debates).

## Bot API Contract

Each bot exposes a single endpoint. The harness does not require or expect anything else.

### Request: `POST /debate`

```json
{
  "session_id": "uuid",
  "round": 0,
  "role": "skeptic",
  "context": [],
  "prompt": "string"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `session_id` | UUID | Unique debate session identifier |
| `round` | integer 0-4 | Current round number |
| `role` | string | Constitutional role assigned for this session |
| `context` | array of objects | Anonymised prior round outputs. Empty in Round 0. Each entry: `{ pseudonym: string, round: int, response: string, confidence: int\|null }` |
| `prompt` | string | Round-specific instruction from the harness |

### Response

```json
{
  "response": "string",
  "confidence": 72,
  "challenge": {
    "claim_targeted": "string",
    "counter_evidence": "string",
    "type": "factual|logical|premise"
  },
  "position_change": {
    "changed": false,
    "from_summary": "string",
    "to_summary": "string",
    "reason": "string"
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `response` | string | Always | Bot's substantive answer |
| `confidence` | integer 0-100 | Round 1+ | Self-assessed confidence. Not required in Round 0. |
| `challenge` | object | Round 2 | Structured challenge. Harness validates presence and completeness in Round 2. Optional in other rounds. |
| `position_change` | object | Round 4 | Explicit declaration of whether and how the bot's position shifted. Required in Round 4. |

### Error Handling

- **Timeout:** 5 minutes per round per bot. Non-responding bots receive an `abstain` entry.
- **Malformed response:** Missing required fields → harness re-prompts with explicit field requirements. Max 2 retries, then force-abstain with validation failure flag.
- **HTTP errors:** 4xx → log and abstain (bot-side issue). 5xx → retry once after 10s, then abstain.
- **Quorum:** Minimum 3 bots must respond per round for the round to be valid. If quorum is lost in Round 0, debate is cancelled. If lost mid-debate, the debate continues with remaining bots; withdrawn bots' last positions are frozen and marked.

### Authentication

Bearer token per bot, issued by the harness operator. Sent as `Authorization: Bearer <token>` on every request. Tokens are stored hashed in the database.

## Debate Protocol

### Round 0 — Blind Formation

Each bot receives the topic and its constitutional role. The `context` array is empty. No bot sees any other's position. All 5 requests are dispatched concurrently.

**Prompt template:**
```
You are participating in a structured adversarial debate.
Topic: {topic}
Your role: {role} — {role_description}

State your initial position on this topic. Be substantive and specific.
Do not hedge or equivocate — commit to a clear position consistent with your assigned role.
```

### Round 1 — Anonymous Distribution

Harness collects all Round 0 responses, strips bot identity, assigns stable pseudonyms (Agent A through E), and redistributes. Each bot receives all 5 anonymised Round 0 positions (including its own, which it must treat as any other).

**Prompt template:**
```
Here are the initial positions from all participants (anonymised).
Your previous position was submitted as {own_pseudonym}.

Review all positions. You must:
1. Identify the single strongest argument that opposes your position and explain why it is strong.
2. State specifically what evidence or reasoning would cause you to change your position.

Do not agree with other positions unless you can articulate exactly why the argument compels agreement.
```

### Round 2 — Structured Rebuttal

Each bot receives the Round 1 responses (anonymised). The harness enforces the mandatory dissent gate: every response must contain a structured `challenge` object with `claim_targeted`, `counter_evidence`, and `type`.

**Validation:** The harness calls MiniMax to verify the challenge is substantive (not a vacuous restatement of disagreement). Validation prompt:
```
Does the following challenge contain a specific factual claim, logical objection, or premise critique directed at a named claim from another participant? Return JSON: { "valid": bool, "reason": "string" }
Challenge: {challenge_json}
Context: {round2_response}
```

Responses failing validation are re-prompted: "Your response was rejected because it did not contain a specific challenge. You must raise at least one factual or logical objection to another participant's position. Resubmit." Max 2 retries.

**Prompt template:**
```
Here are the Round 1 responses from all participants.

You must raise at least one specific challenge. Your challenge must:
- Target a specific claim made by another participant (cite the pseudonym and claim)
- Provide counter-evidence or identify a logical flaw
- Be classified as factual, logical, or premise-based

A response without an explicit challenge will be rejected.
```

### Round 3 — Cross-Examination

Bots are paired by maximum semantic divergence. The harness calls MiniMax with all Round 2 positions:
```
Given these 5 debate positions, identify the two pairs of participants whose positions are most divergent. The remaining participant joins whichever pair has the most similar positions (creating a 3-way). Return JSON: { "pair_1": ["Agent A", "Agent C"], "pair_2": ["Agent B", "Agent E"], "third_joins": "pair_2", "third": "Agent D" }
```

Each bot in a pair receives its partner's full Round 2 position and must: (a) pose one pointed question designed to surface hidden assumptions, and (b) answer the question posed to it. The 3-way group follows a round-robin pattern: A questions B, B questions C, C questions A — each bot poses one and answers one.

**Prompt template (pair):**
```
You are in cross-examination with {partner_pseudonym}.
Their position: {partner_round2_response}

1. Pose one pointed question to {partner_pseudonym} that surfaces a hidden assumption or unstated dependency in their argument.
2. Answer the question posed to you by {partner_pseudonym}: "{partner_question}"

Be direct. Do not soften your question to avoid conflict.
```

**Implementation note:** Cross-examination requires two sub-passes. Pass A: each bot poses its question (dispatched concurrently within pairs). Pass B: each bot answers the question posed to it (dispatched after all Pass A responses are collected).

### Round 4 — Final Position

Each bot states its final position with a confidence score and an explicit `position_change` declaration.

**Prompt template:**
```
This is the final round. State your final position on: {topic}

You must include:
1. Your final position — clear, specific, and substantive.
2. A confidence score (0-100) reflecting your genuine certainty.
3. A position_change declaration: did your position change from Round 0? If yes, state what changed, what it changed from, and the specific argument that caused the change. If no, state why the opposing arguments were insufficient.

Do not soften your position for the sake of agreement. Minority positions are preserved and valued in the synthesis.
```

### Synthesis Pass

After Round 4, the harness runs two steps:

**Step 1 — Deterministic pre-computation (harness, no LLM):**
- Confidence trajectories per bot (Round 1 → Round 4)
- Which bots declared position changes and what they cited
- Challenge graph: who challenged whom, on what, whether the challenge was addressed
- Vote tally: group final positions by substantive alignment

**Step 2 — Divergence analysis (MiniMax, per bot):**

For each bot, MiniMax receives Round 0 and Round 4 positions:
```
Compare these two positions from the same participant in a structured debate.

Round 0 position: {round0_response}
Round 4 position: {round4_response}
Participant's self-declared position_change: {position_change_json}

Assess:
1. Did the position substantively shift? (not just rephrasing)
2. Magnitude: none | minor | major | reversal
3. What specifically changed?
4. Is the participant's self-declared justification adequate — does it cite a specific argument from the debate that accounts for the shift?
5. Any flags (e.g., shift without justification, claimed no change but position clearly different)

Return JSON: { "shifted": bool, "magnitude": "string", "what_changed": "string", "justification_adequate": bool, "flags": ["string"] }
```

**Step 3 — Synthesis (Opus, single call, temperature 0):**

Opus receives the full transcript, all divergence analyses, the pre-computed structural data, and a rigid output schema:

```
You are the synthesis engine for a structured adversarial debate. Your role is analytical, not creative. You must produce a rigorous, citation-backed synthesis.

RULES:
- Every factual claim must cite [Bot pseudonym, Round N].
- Do not infer what a participant "seemed to mean" — use only their stated positions.
- Do not declare consensus unless all participants explicitly agree on the specific point.
- Preserve minority positions with full dignity — a lone dissent with strong reasoning is more valuable than a 4-1 majority with weak reasoning.
- Flag any position shift that lacks adequate justification (from the divergence analysis).

OUTPUT SCHEMA (return valid JSON):
{
  "topic": "string",
  "consensus_points": [
    { "point": "string", "supporting_bots": ["pseudonym"], "evidence": "string [citations]" }
  ],
  "live_disagreements": [
    {
      "issue": "string",
      "side_a": { "position": "string", "bots": ["pseudonym"], "best_argument": "string [citation]" },
      "side_b": { "position": "string", "bots": ["pseudonym"], "best_argument": "string [citation]" }
    }
  ],
  "flagged_capitulations": [
    { "bot": "pseudonym", "from": "string", "to": "string", "justification_adequate": bool, "flag_reason": "string" }
  ],
  "minority_positions": [
    { "bot": "pseudonym", "position": "string", "key_argument": "string [citation]", "confidence": int }
  ],
  "confidence_trajectories": {
    "pseudonym": [null, int, int, int, int]
  },
  "meta_observations": "string — max 200 words, any structural observations about the debate quality itself"
}
```

## Anti-Sycophancy Mechanisms Summary

| Mechanism | Where Enforced | How |
|-----------|---------------|-----|
| Anchoring prevention | Round 0 | Empty context array. Concurrent dispatch. No bot sees others. |
| Confidence laundering prevention | Rounds 1-2 | Bot identity stripped. Stable pseudonyms. Attribution restored only in final synthesis. |
| Cascade prevention | Round 2 | Structured challenge field required. MiniMax validates substantiveness. Re-prompt on failure. |
| Capitulation detection | Post-Round 4 | MiniMax compares Round 0 vs Round 4 per bot. Flags unexplained shifts. |
| False consensus prevention | Synthesis | Opus schema separates consensus / disagreement / capitulation. No implicit agreement. |
| Role enforcement | All rounds | Harness tracks role-consistent behaviour. Premature agreement by Skeptic/DA triggers re-prompt. |

## Data Model (SQLite)

```sql
-- Bot registry
CREATE TABLE bots (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    endpoint_url TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Debate sessions
CREATE TABLE debates (
    id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'created',
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

-- Bot participation in a debate
CREATE TABLE debate_bots (
    debate_id TEXT NOT NULL REFERENCES debates(id),
    bot_id TEXT NOT NULL REFERENCES bots(id),
    role TEXT NOT NULL,
    pseudonym TEXT NOT NULL,
    PRIMARY KEY (debate_id, bot_id)
);

-- Round state
CREATE TABLE rounds (
    debate_id TEXT NOT NULL REFERENCES debates(id),
    round_number INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    started_at TEXT,
    completed_at TEXT,
    PRIMARY KEY (debate_id, round_number)
);

-- Individual bot responses per round
CREATE TABLE responses (
    id TEXT PRIMARY KEY,
    debate_id TEXT NOT NULL REFERENCES debates(id),
    round_number INTEGER NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id),
    response_json TEXT NOT NULL,
    confidence INTEGER,
    challenge_json TEXT,
    position_change_json TEXT,
    valid BOOLEAN NOT NULL DEFAULT true,
    retry_count INTEGER NOT NULL DEFAULT 0,
    abstained BOOLEAN NOT NULL DEFAULT false,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Analysis results (divergence, challenge validation)
CREATE TABLE analyses (
    id TEXT PRIMARY KEY,
    debate_id TEXT NOT NULL REFERENCES debates(id),
    bot_id TEXT,
    analysis_type TEXT NOT NULL,
    input_json TEXT NOT NULL,
    result_json TEXT NOT NULL,
    model_used TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Cross-examination pairings
CREATE TABLE pairings (
    debate_id TEXT NOT NULL REFERENCES debates(id),
    bot_a_id TEXT NOT NULL REFERENCES bots(id),
    bot_b_id TEXT NOT NULL REFERENCES bots(id),
    third_id TEXT REFERENCES bots(id),
    pairing_json TEXT NOT NULL,
    PRIMARY KEY (debate_id, bot_a_id, bot_b_id)
);

-- Final synthesis
CREATE TABLE syntheses (
    debate_id TEXT PRIMARY KEY REFERENCES debates(id),
    output_json TEXT NOT NULL,
    model_used TEXT NOT NULL,
    prompt_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Role rotation history
CREATE TABLE role_history (
    bot_id TEXT NOT NULL REFERENCES bots(id),
    debate_id TEXT NOT NULL REFERENCES debates(id),
    role TEXT NOT NULL,
    PRIMARY KEY (bot_id, debate_id)
);
```

## Project Structure

```
bot-council/
  Cargo.toml
  config/
    default.toml
    production.toml
  migrations/
    001_init.sql
  src/
    main.rs                 -- tokio::main, router assembly, server start
    lib.rs                  -- re-exports, AppState construction
    config.rs               -- Settings struct, TOML + env var loading
    error.rs                -- AppError enum, IntoResponse impl
    state.rs                -- AppState (Arc<Inner> pattern)
    api/
      mod.rs                -- Router::new() assembly
      handlers.rs           -- HTTP handlers (create_debate, get_debate, etc.)
      middleware.rs          -- Bearer token auth extractor
      models.rs             -- Request/response DTOs
    orchestrator/
      mod.rs                -- DebateOrchestrator: spawns and drives debates
      state_machine.rs      -- Round transitions, resumption logic
      prompts.rs            -- All prompt templates (single source of truth)
      roles.rs              -- Role definitions, rotation, assignment
    bot_client/
      mod.rs                -- BotClient: HTTP calls to bot endpoints
      retry.rs              -- Retry policy, timeout handling
    analyser/
      mod.rs                -- MiniMax calls for validation + divergence
      challenge.rs          -- Round 2 challenge validation
      divergence.rs         -- Round 0 vs Round 4 comparison
      pairing.rs            -- Cross-exam divergence pairing
    synthesiser/
      mod.rs                -- Opus synthesis call
      schema.rs             -- Output schema definition and validation
      precompute.rs         -- Deterministic pre-computation (confidence trajectories, challenge graph, vote tally)
    anonymiser/
      mod.rs                -- Strip identity, assign pseudonyms, log mapping
    db/
      mod.rs                -- Pool init, migrations
      models.rs             -- Row types
      queries.rs            -- Query functions
  tests/
    integration/
      api_tests.rs
      orchestrator_tests.rs
      analyser_tests.rs
  reference/
    debate-endpoint-node.js   -- Reference /debate endpoint (Node.js)
    debate-endpoint-python.py -- Reference /debate endpoint (Python)
```

## API Endpoints (v1)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `POST` | `/debates` | Yes | Create debate. Body: `{ topic, bot_ids?, role_overrides? }` |
| `GET` | `/debates` | Yes | List debates. Query: `?status=active&limit=20` |
| `GET` | `/debates/{id}` | Yes | Full debate state, current round, participant roles |
| `GET` | `/debates/{id}/transcript` | Yes | Round-by-round transcript with anonymisation log |
| `GET` | `/debates/{id}/synthesis` | Yes | Final synthesis (404 if not yet complete) |
| `POST` | `/debates/{id}/cancel` | Yes | Cancel in-progress debate |
| `GET` | `/bots` | Yes | List registered bots |
| `POST` | `/bots` | Yes | Register bot. Body: `{ name, endpoint_url, token }` |
| `PATCH` | `/bots/{id}` | Yes | Update bot (endpoint, token, active flag) |
| `DELETE` | `/bots/{id}` | Yes | Deactivate bot (soft delete) |
| `GET` | `/health` | No | Service health + DB connectivity |

## Configuration

```toml
# config/default.toml

[server]
host = "0.0.0.0"
port = 3100

[database]
url = "sqlite:data/council.db?mode=rwc"

[auth]
admin_token = ""  # Override via env: APP__AUTH__ADMIN_TOKEN

[models]
minimax_api_key = ""       # Override via env
minimax_model = "M2.7"
minimax_base_url = "https://api.minimax.chat"
opus_api_key = ""           # Override via env
opus_model = "claude-opus-4-6"

[debate]
default_timeout_secs = 300        # 5 min per bot per round
max_retries = 2                   # Validation failure retries
quorum = 3                        # Minimum bots per round
synthesis_temperature = 0.0       # Opus synthesis temperature

[http_client]
connect_timeout_secs = 5
request_timeout_secs = 300
max_retries = 1                   # HTTP-level retries (separate from validation retries)
retry_delay_secs = 10
```

## Coding Standards

The harness follows these rules (derived from the Clawdbot CLAUDE.md standards, adapted for Rust):

### Structure
- **Maximum file size: 300 lines.** Split before adding.
- **One file, one job.** Single responsibility per module.
- **No duplicate functions.** Search before writing.
- **All constants in `config.rs` or dedicated constants modules.** Zero `std::env` outside config.
- **Clean up after yourself.** Remove dead code in the same commit.

### Architecture
- **Explicit dependency injection.** Constructor params via `AppState`, not global statics.
- **Repository pattern for I/O.** Business logic in orchestrator/analyser never touches SQLite directly — goes through `db::queries`.
- **Dispatch over match chains.** Use enums and match, not if/else chains for state transitions.

### Type Safety
- **Rust edition 2024.** Strict mode. No `unwrap()` in production paths — `?` operator or explicit error handling.
- **Enums for fixed values.** Roles, round states, analysis types — all typed enums with serde derive.
- **Newtype wrappers for IDs.** `DebateId(String)`, `BotId(String)` — prevent mixing.

### Error Handling
- **`thiserror` for domain errors.** Every variant maps to an HTTP status + JSON body.
- **`anyhow` at binary boundary only.** `main.rs` and background task runners.
- **No silent failures.** Every `Result` is handled. No `.ok()` without comment.
- **Log errors with context.** What failed, what the input was, why it matters. Use `tracing` structured fields.

### Async & Testing
- **Concurrent dispatch where independent.** All Round 0 bot calls go out via `join_all`. Sequential only where data dependencies exist.
- **Integration tests for all API endpoints.** `tower::ServiceExt::oneshot` with in-memory SQLite.
- **Unit tests for orchestrator state machine.** Every transition, every edge case (timeout, quorum loss, validation failure).
- **Documented public items.** `///` doc comments on all public functions and types.

### Process
- **CLAUDE.md maintained in real-time.** Every decision, immediately.
- **Commits are atomic.** One logical change per commit. No "WIP" commits on main.
- **Reference implementations tested.** The Node.js and Python reference endpoints must pass a harness integration test.

## Not In Scope (v1)

- Web dashboard / React frontend (separate project)
- WhatsApp integration
- WebSocket real-time updates
- Style normalisation of anonymised responses
- Bot health monitoring / uptime tracking
- Multi-topic or chained debates
- Human participant mode
- Debate templates or topic libraries
