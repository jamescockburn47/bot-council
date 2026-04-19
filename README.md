# LQ Bot Council

A standalone Rust/Axum service that orchestrates structured multi-agent adversarial debates. The harness manages a 5-round protocol with anti-sycophancy mechanisms enforced structurally (not through prompting), produces a rigorous synthesis via a tightly-prompted Opus call, and persists full session state in SQLite.

## What This Is

The Bot Council is a general-purpose debate orchestration harness. It communicates with N participating bots via HTTP, assigns constitutional roles, enforces adversarial discipline through protocol rules, and produces auditable, citation-backed synthesis outputs.

The council's value comes from agent diversity — across model families, personas, and reasoning traditions. The protocol's structural enforcement (blind formation, anonymisation, mandatory dissent gates, capitulation detection) prevents the convergence and sycophancy that plague unconstrained multi-agent chat.

## Architecture

```
                    +-------------------+
                    |   Bot Council     |
                    |   (Rust/Axum)     |
                    |   Port 3100       |
                    +--------+----------+
                             |
              +--------------+--------------+
              |              |              |
         POST /debate   POST /debate   POST /debate
              |              |              |
         +----+----+   +----+----+   +----+----+
         |  Bot 1  |   |  Bot 2  |   |  Bot N  |
         | (any LLM)|  | (any LLM)|  | (any LLM)|
         +---------+   +---------+   +---------+

    MiniMax M2.7 ---- analysis, validation, pairing
    Claude Opus  ---- synthesis (temperature 0, rigid schema)
    SQLite       ---- all state persistence
```

- **Bot-agnostic**: Bots expose a single `POST /debate` endpoint. The harness doesn't know or care what model, memory system, or tool stack any bot uses.
- **Resumable**: Every state transition is persisted. If the harness crashes mid-debate, it resumes from the last completed round.
- **Auditable**: Full round-by-round transcript with anonymisation log, divergence analysis, and synthesis provenance. Every claim in the synthesis cites bot pseudonym + round.

## The 5-Round Protocol

| Round | Name | What Happens |
|-------|------|-------------|
| **0** | Blind Formation | Each bot receives the topic and its constitutional role. No bot sees any other's position. Concurrent dispatch. |
| **1** | Anonymous Distribution | All Round 0 responses are anonymised (Agent A-E) and redistributed. Each bot identifies the strongest opposing argument. |
| **2** | Structured Rebuttal | Mandatory dissent gate: every response must include a structured challenge. MiniMax validates each challenge is substantive (not vacuous). Re-prompt on failure, max 2 retries. |
| **3** | Cross-Examination | MiniMax pairs bots by maximum semantic divergence. Two-pass: (A) each bot poses a pointed question, (B) each bot answers. |
| **4** | Final Position | Each bot states its final position with a confidence score and an explicit position-change declaration. |

After Round 4:
- **Divergence Analysis** (MiniMax): Compares each bot's Round 0 vs Round 4 position. Flags unexplained shifts.
- **Synthesis** (Opus, temperature 0): Produces structured JSON with consensus points, live disagreements, flagged capitulations, minority positions, and confidence trajectories. Every claim must cite [pseudonym, Round N].

## Constitutional Roles

Five roles, one per bot, rotated across debates (no consecutive same-role assignment):

| Role | Function |
|------|----------|
| **Proponent** | Constructs the strongest case for the proposition |
| **Skeptic** | Challenges assumptions and demands evidence |
| **Devil's Advocate** | Argues positions it may not hold to stress-test reasoning |
| **Empiricist** | Demands factual grounding, flags unsupported assertions |
| **Steelman** | Strengthens opposing arguments before engaging them |

## Anti-Sycophancy Mechanisms

| Mechanism | Where | How |
|-----------|-------|-----|
| Anchoring prevention | Round 0 | Empty context, concurrent dispatch |
| Confidence laundering prevention | Rounds 1-2 | Identity stripped, stable pseudonyms |
| Cascade prevention | Round 2 | Structured challenge required, MiniMax validates substantiveness |
| Capitulation detection | Post-Round 4 | MiniMax compares Round 0 vs Round 4, flags unexplained shifts |
| False consensus prevention | Synthesis | Opus schema separates consensus / disagreement / capitulation |

## API

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/bots` | Register a bot |
| `GET` | `/bots` | List active bots |
| `POST` | `/debates` | Create and run a multi-round debate |
| `GET` | `/debates` | List debates (filterable by status) |
| `GET` | `/debates/{id}` | Get debate state and results |
| `GET` | `/debates/{id}/transcript` | Full round-by-round transcript with anonymisation log |
| `GET` | `/debates/{id}/synthesis` | Final synthesis output (404 if not yet complete) |
| `GET` | `/health` | Health check |

### Bot API Contract

Each bot exposes a single endpoint at the URL registered with the council (convention: `POST /debate`). The harness authenticates with `Authorization: Bearer <token>` using the token supplied at registration.

**Request:**
```json
{
  "session_id": "uuid",
  "round": 0,
  "role": "skeptic",
  "context": [
    { "pseudonym": "Agent A", "round": 0, "response": "string", "confidence": null }
  ],
  "prompt": "string"
}
```

`context` is empty in Round 0; populated with anonymised prior-round responses from Round 1 onwards. `confidence` in context entries is `null` for Round 0 responses, an integer 0–100 for subsequent rounds.

**Response:**
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

Per-round requirements:

| Round | Required fields |
|-------|----------------|
| 0 | `response` |
| 1 | `response`, `confidence` (integer 0–100) |
| 2 | `response`, `confidence`, `challenge` |
| 3 | `response`, `confidence` |
| 4 | `response`, `confidence`, `position_change` |

Max response body: 20 KB. Endpoint URL must use `https://` (localhost `http://` only in dev).

## Tech Stack

- **Language**: Rust 2024 edition
- **Framework**: Axum 0.8 + Tokio
- **Database**: SQLite via sqlx 0.8
- **HTTP Client**: reqwest 0.12 with retry middleware
- **LLM APIs**: MiniMax M2.7 (analysis), Claude Opus 4.6 (synthesis)
- **Config**: TOML + environment variable overrides (`APP__*`)

## Project Structure

```
bot-council/
  src/
    main.rs                    -- Entry point
    lib.rs                     -- Module declarations, build_app()
    config.rs                  -- Settings (TOML + env vars)
    error.rs                   -- AppError enum with IntoResponse
    types.rs                   -- DebateId, BotId, Role, DebateStatus, RoundStatus
    state.rs                   -- AppState (Arc<Inner>)
    api/
      mod.rs                   -- Router assembly
      auth.rs                  -- BearerAuth extractor
      bots.rs                  -- Bot registration endpoints
      debates.rs               -- Debate CRUD endpoints
      transcript.rs            -- GET /debates/{id}/transcript
      synthesis.rs             -- GET /debates/{id}/synthesis
      dto.rs                   -- Request/response types
      health.rs                -- Health check
    orchestrator/
      mod.rs                   -- Phase 0 run_debate (backward compat)
      multi_round.rs           -- Phase 1 full 5-round driver
      state_machine.rs         -- Round lifecycle + resumption
      roles.rs                 -- Role assignment with rotation
      prompts.rs               -- All prompt templates
      anonymiser.rs            -- Pseudonym assignment
      rounds/
        round0.rs - round4.rs  -- Per-round execution
    analyser/
      mod.rs                   -- MiniMax client
      challenge.rs             -- Round 2 challenge validation
      pairing.rs               -- Round 3 divergence pairing
      divergence.rs            -- Post-Round 4 position comparison
    synthesiser/
      mod.rs                   -- Opus synthesis call
      precompute.rs            -- Deterministic pre-computation
      schema.rs                -- Synthesis output types
    bot_client/
      mod.rs                   -- HTTP client for bot communication
    db/
      mod.rs                   -- Pool init + migrations
      models.rs                -- Row structs
      queries.rs               -- Phase 0 SQL functions
      queries_phase1.rs        -- Phase 1 SQL functions
  migrations/
    20260415000001_init.sql    -- Phase 0 schema
    20260415000002_phase1.sql  -- Phase 1 additions
  config/
    default.toml               -- Default configuration
  tests/
    common/mod.rs              -- Test helpers
    api_bots_test.rs           -- Bot endpoint tests
    api_debates_test.rs        -- Debate endpoint tests (incl. transcript/synthesis)
    api_health_test.rs         -- Health check test
  reference/
    debate-endpoint-node.js    -- Reference bot (Node.js, port 3200)
    debate-endpoint-python.py  -- Reference bot (Python, port 3201)
    run-smoke-test.sh          -- E2E smoke test
```

## Setup

### Prerequisites

- Rust 2024 edition (rustup default nightly or stable with edition = "2024")
- SQLite 3
- MiniMax API key (for analysis/validation)
- Anthropic API key (for Opus synthesis)

### Configuration

Copy and edit `config/default.toml`, or use environment variables:

```bash
export APP__MODELS__MINIMAX_API_KEY="your-minimax-key"
export APP__MODELS__OPUS_API_KEY="your-anthropic-key"
export APP__AUTH__ADMIN_TOKEN="your-admin-token"
```

### Build and Run

```bash
cargo build --release
cargo run
# Listening on 0.0.0.0:3100
```

### Run Tests

```bash
cargo test
# 8 tests: bots (2), debates (5), health (1)
```

### Quick Start with Reference Bots

```bash
# Terminal 1: Start the harness
cargo run

# Terminal 2: Start reference bots
node reference/debate-endpoint-node.js &   # Port 3200
python reference/debate-endpoint-python.py & # Port 3201

# Terminal 3: Run smoke test
bash reference/run-smoke-test.sh
```

## Phased Build Plan

| Phase | Status | Description |
|-------|--------|-------------|
| **0** | Complete | Single-shot MVP: connectivity, anonymisation, peer scoring |
| **1** | Complete | Multi-round protocol: roles, Rounds 0-4, MiniMax validation, Opus synthesis |
| **2** | Planned | Judge model, reputation/Elo, diversity tracking |
| **3** | Planned | LQ Brain (shared knowledge layer), platform hardening |

## Documentation

- **[BOT_AUTHORING.md](BOT_AUTHORING.md)**: authoritative end-to-end reference for anyone building a bot. Protocol, schemas, rounds, roles, testing flow, error taxonomy with remediation, LLM-wrapping pattern, FAQ. Start here if you're writing a bot.
- **[ARCHITECTURE.md](ARCHITECTURE.md)**: forensic deployment topology, regression-prevention contract, release procedures.
- **[INTEGRATIONS.md](INTEGRATIONS.md)**: ops playbook — EVO/Tailscale/systemd/Vercel/Clerk/Sentry seams and how to keep them in repair.
- **[CLAUDE.md](CLAUDE.md)**: development protocols for Claude Code working in this repo (coding standards, branch hygiene, release gate).
- **Design Spec**: `docs/superpowers/specs/2026-04-15-bot-council-harness-design.md`
- **Phase 0 Plan**: `docs/superpowers/plans/2026-04-15-phase0-single-shot-mvp.md`
- **Phase 1 Plan**: `docs/superpowers/plans/2026-04-15-phase1-multi-round-protocol.md`

## Community Context

Built for the LQ community. The harness is general-purpose — not coupled to any specific bot, messaging platform, or the Clawdbot codebase. Community feedback from LQ_Alice and Artur Serov is incorporated in the design spec.

## License

Private. Contact James Cockburn for access.
