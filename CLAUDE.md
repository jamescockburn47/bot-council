# CLAUDE.md — LQ Bot Council Harness

## Quick Reference

| Key | Value |
|-----|-------|
| **Language** | Rust 2024 edition |
| **Framework** | Axum 0.8, Tokio |
| **Database** | SQLite via sqlx 0.8 |
| **Port** | 3100 |
| **Config** | config/default.toml + APP__* env vars |
| **Build/Test host** | EVO X2 (`james@100.90.66.54:~/bot-council`) |
| **Run** | `cargo run` (on EVO) |
| **Test** | `cargo test` (on EVO) |
| **Spec** | `docs/superpowers/specs/2026-04-15-bot-council-harness-design.md` |
| **Plan (P0)** | `docs/superpowers/plans/2026-04-15-phase0-single-shot-mvp.md` |
| **Plan (P1)** | `docs/superpowers/plans/2026-04-15-phase1-multi-round-protocol.md` |

## Deploy Workflow

Edit locally, sync to EVO, build there:
```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests config migrations Cargo.toml Cargo.lock james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

## Coding Standards — BINDING

- Max 300 lines per file. Split before adding.
- One file, one job. Single responsibility.
- No `unwrap()` in production paths.
- No `.ok()` without `// intentional: [reason]` comment.
- Newtype wrappers for IDs: `DebateId(String)`, `BotId(String)`.
- Enums with serde derive for fixed values.
- All config in `config.rs`. Zero `std::env` outside config.
- Repository pattern: handlers call `db::queries`, never raw SQL.
- `thiserror` for domain errors, `anyhow` at binary boundary only.
- Tracing with structured fields for all error logging.
- `join_all` for concurrent independent operations.
- Integration tests via `tower::ServiceExt::oneshot` with in-memory SQLite.
- `///` doc comments on all public items.
- Atomic commits. One logical change per commit.

## Architecture

Standalone Rust/Axum service. No dependency on Clawdbot or any specific bot.
Communicates with bots via HTTP POST to their /debate endpoint.
Persists all state in SQLite. Background Tokio tasks run debates asynchronously.

## Current Phase: 1 (Multi-Round Protocol)

Phase 1 supports: constitutional roles with rotation, 5-round adversarial protocol
(blind formation, anonymous distribution, structured rebuttal with MiniMax validation,
cross-examination with MiniMax pairing, final position with position change tracking),
divergence analysis, Opus synthesis. State machine with resumption from any round.

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | /bots | Register a bot |
| GET | /bots | List active bots |
| POST | /debates | Create and run a debate |
| GET | /debates | List debates |
| GET | /debates/{id} | Get debate with results |
| GET | /debates/{id}/transcript | Full transcript with anonymisation log |
| GET | /debates/{id}/synthesis | Synthesis output (404 if incomplete) |
| GET | /health | Health check |
