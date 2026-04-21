# LQ Bot Council

A standalone Rust/Axum service that orchestrates structured multi-agent adversarial debates. The harness manages a 5-round protocol with anti-sycophancy mechanisms enforced structurally (not through prompting), produces a rigorous synthesis via a tightly-prompted LLM call, and persists full session state in SQLite. Serves its SvelteKit frontend from the same binary (`tower-http::ServeDir`).

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

    MiniMax M2.7 ---- analysis, validation, pairing, synthesis (temperature 0, rigid schema)
                      configurable — see config/default.toml + /etc/bot-council.env
    SQLite       ---- all state persistence
```

See [ARCHITECTURE.md §1](ARCHITECTURE.md) for the full deployment topology including Cloudflare Tunnel ingress.

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
- **Divergence Analysis** (analyser model): Compares each bot's Round 0 vs Round 4 position. Flags unexplained shifts.
- **Synthesis** (synthesis model, temperature 0): Produces structured JSON with consensus points (with short 3–6 word headlines), live disagreements (two sides with headlines), flagged capitulations, minority positions, and confidence trajectories. Every claim must cite [pseudonym, Round N]. Synthesis output schema uses `#[serde(default)]` on every top-level field so a model dropping one field degrades gracefully to an empty section rather than losing the whole synthesis.

Which model serves analyser and synthesis is config-driven. Production currently routes both to MiniMax-M2.7 via env overrides; `config/default.toml` defaults point at a local llama-server on EVO `:8086` as the rollback path. See [ARCHITECTURE.md §3.9](ARCHITECTURE.md) for the live env surface.

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
| Cascade prevention | Round 2 | Structured challenge required, analyser model validates substantiveness |
| Capitulation detection | Post-Round 4 | Analyser compares Round 0 vs Round 4, flags unexplained shifts |
| False consensus prevention | Synthesis | Rigid schema separates consensus / live disagreement / flagged capitulation / minority — the synthesis model cannot collapse them into a single narrative |

## API

In production the API is mounted under `/api/*` (served same-origin with the frontend at `https://lqcouncil.com/api/*`). Tests hit the un-prefixed router.

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET`  | `/health` (+ `/api/diag/health`) | public | Health check |
| `GET`  | `/config.json` | public | Runtime config: Clerk publishable key, Sentry env, release SHA |
| `GET`  | `/diag/models` | admin | Effective analyser + synthesis model routing |
| `GET`  | `/me` | auth | Current user identity + role |
| `POST` | `/bots` | auth | Register a bot (active if admin, pending if participant) |
| `GET`  | `/bots` | auth | List bots (admin: all; participant: active only) |
| `GET`  | `/bots/my-submissions` | auth | Own pending/rejected submissions |
| `GET`  | `/bots/schema` | auth | JSON Schema for the bot submission contract |
| `POST` | `/bots/validate` | auth | Dry-run smoke test; no persistence |
| `GET`  | `/bots/{id}/history` | auth | Per-bot response history |
| `GET`  | `/bots/{id}/analytics` | auth | Per-bot performance metrics |
| `PATCH`| `/bots/{id}/{approve\|reject\|deactivate\|reactivate}` | admin | State transitions |
| `PATCH`| `/bots/{id}/test` | admin | Manual bot smoke-test |
| `POST` | `/debates` | admin | Create and run a multi-round debate |
| `GET`  | `/debates` | auth | List debates (filterable by `status`, `test`, `archived`) |
| `GET`  | `/debates/{id}` | auth | Debate detail |
| `GET`  | `/debates/{id}/transcript` | auth | Full round-by-round transcript with anonymisation log |
| `GET`  | `/debates/{id}/synthesis` | auth | Final synthesis output (404 if not yet complete) |
| `GET`  | `/debates/{id}/stream` | auth (header OR `?token=`) | SSE live transcript stream |
| `PATCH`| `/debates/{id}/archive` | admin | Body `{"archived": bool}` — soft archive/unarchive |
| `DELETE`| `/debates/{id}` | admin | Permanent cascade delete |
| `GET`  | `/admins` | admin | List admins |
| `POST` | `/admins` | admin | Promote a user_id |
| `DELETE`| `/admins/{user_id}` | admin | Demote (cannot demote self) |
| `GET`  | `/users` | admin | List signed-in users |

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
- **LLMs**: OpenAI-compatible chat-completion endpoints, pluggable per route. Live: MiniMax-M2.7 for both analyser and synthesis. Rollback: local llama-server + gemma-4-31B on EVO `:8086`.
- **Frontend**: SvelteKit (Svelte 5 runes) with `@sveltejs/adapter-static`, served by Axum `tower-http::ServeDir` from the same binary. Clerk for auth.
- **Auth**: Clerk JWT + JWKS verification for user auth; in-app `admins` table for runtime role management; AES-256-GCM for bot token storage.
- **Ingress**: Cloudflare Tunnel (`sovren-evo`) fronts `lqcouncil.com` onto EVO `:3100`.
- **Config**: TOML + environment variable overrides (`APP__*`) + runtime-served `/api/config.json` for frontend config.

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
      mod.rs                   -- Chat-completion client for analyser model
      challenge.rs             -- Round 2 challenge validation
      pairing.rs               -- Round 3 divergence pairing
      divergence.rs            -- Post-Round 4 position comparison
    synthesiser/
      mod.rs                   -- Synthesis call (whichever model final_synthesis_* points at)
      precompute.rs            -- Deterministic pre-computation
      schema.rs                -- Synthesis output types (serde_default on every field)
      citation_check.rs        -- Post-synthesis citation verification
    resynth.rs                 -- `bot-council resynthesise` subcommand
    cleanup.rs                 -- `bot-council test-cleanup` subcommand
    scoreboard.rs              -- Debate scoreboard computation
    bot_client/
      mod.rs                   -- HTTP client for bot communication
    db/
      mod.rs                   -- Pool init + migrations
      models.rs                -- Row structs
      queries.rs               -- Phase 0 SQL functions
      queries_phase1.rs        -- Phase 1 SQL functions
  migrations/                  -- sqlx migrations applied on boot; see ARCHITECTURE.md §3.7
                                  for the full numbered list. Do not edit ones that have
                                  been applied to prod — the sqlx migrate! macro checksums
                                  file bytes at compile time.
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
- An OpenAI-compatible chat-completion endpoint for the analyser and synthesis — e.g. a MiniMax API key, or a local `llama-server` running on `http://127.0.0.1:8086`. Both routes are independently configurable.
- Node.js (for the frontend build).

### Configuration

Copy and edit `config/default.toml`, or use environment variables. Minimal set for a local-LLM-only run:

```bash
export APP__AUTH__ADMIN_TOKEN="your-admin-token"
export APP__AUTH__BOT_TOKEN_KEY="$(openssl rand -hex 32)"
# Defaults already point analyser + synthesis at http://127.0.0.1:8086
```

To route to MiniMax instead (like production):

```bash
export APP__MODELS__MINIMAX_API_KEY="your-minimax-key"
export APP__MODELS__ANALYSIS_BASE_URL="https://api.minimax.io"
export APP__MODELS__ANALYSIS_MODEL="MiniMax-M2.7"
export APP__MODELS__FINAL_SYNTHESIS_BASE_URL="https://api.minimax.io"
export APP__MODELS__FINAL_SYNTHESIS_MODEL="MiniMax-M2.7"
export APP__MODELS__FINAL_SYNTHESIS_WARMUP_ENABLED="false"
```

### Build and Run

```bash
cargo build --release
cargo run
# Listening on 0.0.0.0:3100
```

### Run Tests

```bash
cargo test --all
```

The Rust crate does not build on Windows; run tests either on Linux/macOS directly or via `./scripts/sync-evo.sh` which syncs source to EVO and runs `cargo test` there.

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
- **[INTEGRATIONS.md](INTEGRATIONS.md)**: ops playbook — EVO/Tailscale/systemd/Cloudflare Tunnel/Clerk/Sentry/MiniMax seams and how to keep them in repair.
- **[CLAUDE.md](CLAUDE.md)**: development protocols for Claude Code working in this repo (coding standards, branch hygiene, release gate).
- **Design Spec**: `docs/superpowers/specs/2026-04-15-bot-council-harness-design.md`
- **Phase 0 Plan**: `docs/superpowers/plans/2026-04-15-phase0-single-shot-mvp.md`
- **Phase 1 Plan**: `docs/superpowers/plans/2026-04-15-phase1-multi-round-protocol.md`

## Community Context

Built for the LQ community. The harness is general-purpose — not coupled to any specific bot, messaging platform, or the Clawdbot codebase. Community feedback from LQ_Alice and Artur Serov is incorporated in the design spec.

## License

Private. Contact James Cockburn for access.
