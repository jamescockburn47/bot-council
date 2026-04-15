# Bot Council Frontend — Continuation Instructions for New Agent

> Read this file, then read the files listed below in order. Do not start work until you have read all of them.

## What This Project Is

The LQ Bot Council is a Rust/Axum service (port 3100) that orchestrates structured adversarial debates between AI bots. The **backend is complete** (Phase 0 + Phase 1). Your job is to design and build a **separate web frontend** that provides a visual interface for the council.

## Current State: Backend Complete, No Frontend

The backend API is live and tested. It can:
- Register bots (`POST /bots`), list bots (`GET /bots`)
- Create multi-round debates (`POST /debates`) with 5-round adversarial protocol
- Return full transcripts (`GET /debates/{id}/transcript`) with round-by-round anonymised responses
- Return synthesis output (`GET /debates/{id}/synthesis`) — structured JSON from Opus
- List and filter debates (`GET /debates?status=complete`)

There is currently no frontend at all — everything is curl/API only.

## Files You Must Read (in order)

1. **`CLAUDE.md`** — Project-level coding standards and architecture overview
2. **`README.md`** — Full architecture, API reference, protocol description
3. **`src/api/dto.rs`** — All API response shapes (this is what the frontend consumes)
4. **`docs/superpowers/specs/2026-04-15-bot-council-harness-design.md`** — Full design spec, especially the Debate Protocol section (Rounds 0-4) and Synthesis Pass

## What to Build

A **separate web frontend** (its own directory, its own build, its own deployment) that talks to the Axum backend API. This is NOT served from Axum — it's a standalone SPA deployed separately so external users can access it.

### Phase 1.5a: Debate Viewer + Admin (BUILD THIS FIRST)

The core SPA with these capabilities:

**Debate Viewer:**
- Visual representation of the debate process — show rounds unfolding with anonymised responses, challenges, position changes, confidence trajectories
- Round-by-round transcript display with role indicators and challenge/rebuttal highlighting
- Synthesis view: consensus points, live disagreements, flagged capitulations, minority positions
- Debate history: browse and reference previous debates, filter by status, compare outcomes

**Admin Panel:**
- Bot admission: register new bots, view active bots, activate/deactivate
- Debate creation: pick topic, select bots, configure and launch
- Debate queue: view running debates, their current round status
- Protocol configuration: editable number of rounds, role definitions, custom prompt templates per round, variable number of bots per debate (not locked to 5)
- Debate goal modes: selectable objectives — consensus-seeking, winner-takes-all, devil's advocate stress test, and others

**Auth:**
- Start simple: single shared token matching the API's bearer token auth
- Design for evolution toward proper user accounts later

### Phase 1.5b: Live Stream (AFTER 1.5a)

- Real-time view of debates in progress that spectators can watch
- Requires backend changes: WebSocket or SSE endpoint to push round-by-round updates
- Builds on the debate viewer from 1.5a

### Backend Changes Needed

The current backend API does NOT support everything the frontend needs. You will need to add:

1. **CORS headers** — the frontend is on a different origin
2. **Configurable debate parameters** — the API currently hardcodes 5 rounds. Need to accept round count, role overrides, custom prompts, and debate goal mode in `CreateDebateRequest`
3. **WebSocket/SSE endpoint** (Phase 1.5b) — for live streaming round updates
4. **Bot PATCH/DELETE endpoints** — exist in the spec but not yet implemented

### Key Design Constraints

- **Separate deployment** — the frontend must be independently deployable (not bundled into the Rust binary)
- **Web-accessible** — external users log in and watch, not just the operator
- **The backend runs on Evo X2** (`james@100.90.66.54:3100` via Tailscale) — the frontend can be deployed anywhere that can reach it
- **The API returns JSON** — all the shapes are in `src/api/dto.rs`
- **Anonymisation is real** — the frontend displays pseudonyms (Agent A-E), not bot names. The anonymisation log maps pseudonym to role, but NOT to bot identity (that's admin-only)

### API Response Shapes (Summary)

**GET /debates/{id}/transcript:**
```json
{
  "debate_id": "uuid",
  "topic": "string",
  "rounds": [
    {
      "round_number": 0,
      "status": "complete",
      "responses": [
        {
          "pseudonym": "Agent A",
          "response": "text",
          "confidence": 72,
          "challenge": { "claim_targeted": "...", "counter_evidence": "...", "type": "factual" },
          "position_change": { "changed": false, "from_summary": "...", "to_summary": "...", "reason": "..." },
          "valid": true,
          "abstained": false
        }
      ]
    }
  ],
  "anonymisation_log": [
    { "pseudonym": "Agent A", "role": "skeptic" }
  ]
}
```

**GET /debates/{id}/synthesis:**
```json
{
  "debate_id": "uuid",
  "synthesis": {
    "topic": "string",
    "consensus_points": [...],
    "live_disagreements": [...],
    "flagged_capitulations": [...],
    "minority_positions": [...],
    "confidence_trajectories": { "Agent A": [null, 65, 70, 68, 72] },
    "meta_observations": "string"
  },
  "model_used": "claude-opus-4-6",
  "created_at": "timestamp"
}
```

### Community Context

This is for the LQ (Liquid Questions) community. The audience includes:
- **Operators** (James) — full admin access, bot management, debate configuration
- **Community members** (LQ_Alice, Artur, others) — spectators who want to watch debates unfold and browse results
- **Bot operators** — people who run their own bots and want to see how they perform

### Suggested Approach

1. Use the **brainstorming skill** (`superpowers:brainstorming`) to design the frontend
2. Pick a framework (React/Vue/Svelte/etc.) — consider what deploys easily and James is comfortable maintaining
3. Write a design spec, get approval
4. Use **writing-plans** skill to create an implementation plan
5. Build Phase 1.5a first, then 1.5b

### Build and Deploy

The backend builds on Evo X2 (AMD Strix Halo, 128GB). The frontend should be buildable locally (Windows) or on Evo. Deploy target TBD during brainstorming — could be Evo (served by nginx), Vercel, Netlify, or any static host.

```bash
# Backend is at:
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54
# API: http://100.90.66.54:3100 (Tailscale)

# GitHub repo:
# https://github.com/jamescockburn47/bot-council
```
