# Phase 1: Multi-Round Adversarial Protocol — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the full 5-round adversarial debate protocol (Rounds 0-4) with constitutional roles, MiniMax validation/analysis, Opus synthesis, state machine with resumption, and transcript API — on top of the Phase 0 infrastructure.

**Architecture:** The Phase 0 single-shot orchestrator (`run_debate`) is replaced by a round-based state machine. Each round is a separate function. The orchestrator drives rounds sequentially, persisting state after each. New modules: `roles` (role definitions + assignment), `prompts` (all prompt templates), `analyser` (MiniMax calls for challenge validation, pairing, divergence), `synthesiser` (Opus call + pre-computation). The bot API contract gains `role`, `challenge`, and `position_change` fields — backward-compatible (all new fields optional for Phase 0 bots, required only in specific rounds).

**Tech Stack:** Rust 2024, Axum 0.8, Tokio, sqlx 0.8 (SQLite), reqwest 0.12, serde, thiserror, tracing. New: `rand` 0.9 (role shuffling), `reqwest` direct calls for MiniMax/Opus API (no middleware needed for LLM calls).

**Coding rules (from CLAUDE.md — binding on all tasks):**
- Max 300 lines per file. Split before adding.
- One file, one job. Single responsibility.
- No `unwrap()` in production paths. `?` operator or explicit handling.
- No `.ok()` without `// intentional: [reason]` comment.
- Newtype wrappers for IDs: `DebateId(String)`, `BotId(String)`.
- Enums for fixed values with serde derive.
- All config in `config.rs`. Zero `std::env` outside config.
- Repository pattern: handlers call `db::queries`, never raw SQL.
- `thiserror` for domain errors, `anyhow` at binary boundary only.
- Tracing with structured fields for all error logging.
- `join_all` for concurrent independent operations.
- Integration tests via `tower::ServiceExt::oneshot` with in-memory SQLite.
- `///` doc comments on all public items.
- Atomic commits. One logical change per commit.

**Deploy workflow:**
```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests config migrations Cargo.toml Cargo.lock james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

---

## File Structure

### New Files

```
src/
  orchestrator/
    roles.rs             -- Role enum, descriptions, assignment with rotation (~80 lines)
    prompts.rs           -- All prompt templates for Rounds 0-4 (~120 lines)
    state_machine.rs     -- RoundState enum, transitions, resumption logic (~100 lines)
    rounds/
      mod.rs             -- Re-exports round runner functions (~10 lines)
      round0.rs          -- Blind formation dispatch (~80 lines)
      round1.rs          -- Anonymous distribution + strongest-opposing-arg (~90 lines)
      round2.rs          -- Structured rebuttal + validation loop (~120 lines)
      round3.rs          -- Cross-examination two-pass (~130 lines)
      round4.rs          -- Final position + position_change (~80 lines)
  analyser/
    mod.rs               -- LlmClient trait + MiniMax client impl (~80 lines)
    challenge.rs         -- Round 2 challenge validation (~60 lines)
    pairing.rs           -- Round 3 divergence pairing (~70 lines)
    divergence.rs        -- Post-Round 4 per-bot divergence analysis (~70 lines)
  synthesiser/
    mod.rs               -- Opus synthesis call (~80 lines)
    precompute.rs        -- Deterministic pre-computation (~100 lines)
    schema.rs            -- Synthesis output types (~80 lines)
  api/
    transcript.rs        -- GET /debates/{id}/transcript handler (~80 lines)
    synthesis.rs         -- GET /debates/{id}/synthesis handler (~50 lines)
migrations/
  20260415000002_phase1.sql  -- New tables: rounds, analyses, pairings, syntheses, role_history (~60 lines)
```

### Modified Files

```
src/
  lib.rs               -- Add analyser, synthesiser module declarations
  config.rs            -- Add ModelsConfig (minimax/opus keys + model names), DebateConfig
  state.rs             -- No changes (analyser/synthesiser use pool + reqwest directly)
  types.rs             -- Add RoundNumber newtype, RoundStatus enum, Role enum (or in roles.rs)
  error.rs             -- Add AnalyserError, SynthesisError variants
  orchestrator/
    mod.rs             -- Replace run_debate with run_multi_round_debate
    anonymiser.rs      -- No changes (pseudonym assignment unchanged)
  bot_client/
    mod.rs             -- Add DebateRequest/DebateResponse with role/challenge/position_change fields
  db/
    models.rs          -- Add RoundRow, AnalysisRow, PairingRow, SynthesisRow, RoleHistoryRow
    queries.rs         -- Add round/analysis/pairing/synthesis/role_history CRUD
  api/
    mod.rs             -- Add transcript + synthesis routes
    debates.rs         -- Update create_debate to assign roles, use multi-round orchestrator
    dto.rs             -- Add TranscriptResponse, SynthesisResponse DTOs
  tests/
    common/mod.rs      -- Update test_app to include new config fields
config/
  default.toml         -- Add [models] and [debate] sections
Cargo.toml             -- Add rand = "0.9"
```

---

## Task 1: Database Migration — Phase 1 Tables

**Files:**
- Create: `migrations/20260415000002_phase1.sql`

- [ ] **Step 1: Write the Phase 1 migration**

```sql
-- migrations/20260415000002_phase1.sql

-- Round state tracking (resumable state machine)
CREATE TABLE IF NOT EXISTS rounds (
    debate_id TEXT NOT NULL REFERENCES debates(id),
    round_number INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    started_at TEXT,
    completed_at TEXT,
    PRIMARY KEY (debate_id, round_number)
);

-- Add role column to debate_bots (nullable for Phase 0 backward compat)
ALTER TABLE debate_bots ADD COLUMN role TEXT;

-- Add Phase 1 columns to responses (all nullable for backward compat)
ALTER TABLE responses ADD COLUMN confidence INTEGER;
ALTER TABLE responses ADD COLUMN challenge_json TEXT;
ALTER TABLE responses ADD COLUMN position_change_json TEXT;
ALTER TABLE responses ADD COLUMN valid INTEGER NOT NULL DEFAULT 1;
ALTER TABLE responses ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;

-- Analysis results (challenge validation, divergence, pairing)
CREATE TABLE IF NOT EXISTS analyses (
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
CREATE TABLE IF NOT EXISTS pairings (
    debate_id TEXT NOT NULL REFERENCES debates(id),
    bot_a_id TEXT NOT NULL REFERENCES bots(id),
    bot_b_id TEXT NOT NULL REFERENCES bots(id),
    third_id TEXT REFERENCES bots(id),
    pairing_json TEXT NOT NULL,
    PRIMARY KEY (debate_id, bot_a_id, bot_b_id)
);

-- Final synthesis output
CREATE TABLE IF NOT EXISTS syntheses (
    debate_id TEXT PRIMARY KEY REFERENCES debates(id),
    output_json TEXT NOT NULL,
    model_used TEXT NOT NULL,
    prompt_hash TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Role rotation history (prevents same role in consecutive debates)
CREATE TABLE IF NOT EXISTS role_history (
    bot_id TEXT NOT NULL REFERENCES bots(id),
    debate_id TEXT NOT NULL REFERENCES debates(id),
    role TEXT NOT NULL,
    PRIMARY KEY (bot_id, debate_id)
);
```

- [ ] **Step 2: Verify migration applies on top of existing schema**

Sync to Evo and test:
```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r migrations james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

Expected: all 6 existing tests still pass, migration applies without error.

- [ ] **Step 3: Commit**

```bash
git add migrations/20260415000002_phase1.sql
git commit -m "db: add Phase 1 tables — rounds, analyses, pairings, syntheses, role_history"
```

---

## Task 2: Config — Add Models and Debate Sections

**Files:**
- Modify: `src/config.rs`
- Modify: `config/default.toml`
- Modify: `tests/common/mod.rs`

- [ ] **Step 1: Add ModelsConfig and DebateConfig to config.rs**

Add after the `HttpClientConfig` struct in `src/config.rs`:

```rust
/// LLM model configuration for MiniMax (analysis) and Opus (synthesis).
#[derive(Debug, Deserialize, Clone)]
pub struct ModelsConfig {
    pub minimax_api_key: String,
    pub minimax_model: String,
    pub minimax_base_url: String,
    pub opus_api_key: String,
    pub opus_model: String,
}

/// Debate protocol tuning.
#[derive(Debug, Deserialize, Clone)]
pub struct DebateConfig {
    pub default_timeout_secs: u64,
    pub max_retries: u32,
    pub quorum: usize,
    pub synthesis_temperature: f64,
}
```

Add `models` and `debate` fields to the `Settings` struct:

```rust
pub struct Settings {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub http_client: HttpClientConfig,
    pub models: ModelsConfig,
    pub debate: DebateConfig,
}
```

- [ ] **Step 2: Update config/default.toml**

Add at the end of `config/default.toml`:

```toml
[models]
minimax_api_key = ""
minimax_model = "M2.7"
minimax_base_url = "https://api.minimax.chat"
opus_api_key = ""
opus_model = "claude-opus-4-6"

[debate]
default_timeout_secs = 300
max_retries = 2
quorum = 3
synthesis_temperature = 0.0
```

- [ ] **Step 3: Update test helper to include new config fields**

In `tests/common/mod.rs`, add to the `Settings` construction:

```rust
let settings = Settings {
    server: ServerConfig { host: "127.0.0.1".into(), port: 0 },
    database: DatabaseConfig { url: "sqlite::memory:".into() },
    auth: AuthConfig { admin_token: "".into() },
    http_client: HttpClientConfig {
        connect_timeout_secs: 5,
        request_timeout_secs: 30,
        max_retries: 0,
        retry_delay_secs: 1,
    },
    models: ModelsConfig {
        minimax_api_key: "test-minimax-key".into(),
        minimax_model: "M2.7".into(),
        minimax_base_url: "http://localhost:9999".into(),
        opus_api_key: "test-opus-key".into(),
        opus_model: "claude-opus-4-6".into(),
    },
    debate: DebateConfig {
        default_timeout_secs: 30,
        max_retries: 2,
        quorum: 3,
        synthesis_temperature: 0.0,
    },
};
```

- [ ] **Step 4: Sync and verify all existing tests pass**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests config Cargo.toml james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs config/default.toml tests/common/mod.rs
git commit -m "config: add models (MiniMax/Opus) and debate protocol settings"
```

---

## Task 3: Types — Role Enum, RoundStatus, Extended DebateStatus

**Files:**
- Modify: `src/types.rs`

- [ ] **Step 1: Add Role enum to types.rs**

Add after `DebateStatus`:

```rust
/// Constitutional debate roles assigned to bots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Proponent,
    Skeptic,
    DevilsAdvocate,
    Empiricist,
    Steelman,
}

impl Role {
    /// All five constitutional roles.
    pub const ALL: [Role; 5] = [
        Role::Proponent,
        Role::Skeptic,
        Role::DevilsAdvocate,
        Role::Empiricist,
        Role::Steelman,
    ];

    /// Canonical string for database storage.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Proponent => "proponent",
            Self::Skeptic => "skeptic",
            Self::DevilsAdvocate => "devils_advocate",
            Self::Empiricist => "empiricist",
            Self::Steelman => "steelman",
        }
    }

    /// Parse from database string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "proponent" => Some(Self::Proponent),
            "skeptic" => Some(Self::Skeptic),
            "devils_advocate" => Some(Self::DevilsAdvocate),
            "empiricist" => Some(Self::Empiricist),
            "steelman" => Some(Self::Steelman),
            _ => None,
        }
    }

    /// Human-readable description of the role for prompt injection.
    pub fn description(&self) -> &str {
        match self {
            Self::Proponent => "Constructs the strongest case for the proposition",
            Self::Skeptic => "Challenges assumptions and demands evidence",
            Self::DevilsAdvocate => "Argues positions it may not hold to stress-test reasoning",
            Self::Empiricist => "Demands factual grounding, flags unsupported assertions",
            Self::Steelman => "Strengthens opposing arguments before engaging them",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
```

- [ ] **Step 2: Add RoundStatus enum**

Add after the `Role` impl block:

```rust
/// Status of a single round within a debate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundStatus {
    Pending,
    InProgress,
    Complete,
    Failed,
}

impl RoundStatus {
    /// Canonical string for database storage.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }

    /// Parse from database string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "in_progress" => Some(Self::InProgress),
            "complete" => Some(Self::Complete),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}
```

- [ ] **Step 3: Extend DebateStatus with round-based states**

Add new variants to `DebateStatus`:

```rust
pub enum DebateStatus {
    Created,
    Dispatching,
    Scoring,       // Phase 0 only — kept for backward compat
    Round0,
    Round1,
    Round2,
    Round3,
    Round4,
    Analysing,     // Post-round-4 divergence analysis
    Synthesising,  // Opus synthesis pass
    Complete,
    Cancelled,
    Failed,
}
```

Update `as_str()` and `from_str()` to handle the new variants:

```rust
Self::Round0 => "round_0",
Self::Round1 => "round_1",
Self::Round2 => "round_2",
Self::Round3 => "round_3",
Self::Round4 => "round_4",
Self::Analysing => "analysing",
Self::Synthesising => "synthesising",
```

- [ ] **Step 4: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

Expected: 6 tests pass (no breaking changes — new variants added, old ones retained).

- [ ] **Step 5: Commit**

```bash
git add src/types.rs
git commit -m "types: add Role, RoundStatus enums and round-based DebateStatus variants"
```

---

## Task 4: DB Models and Queries — Phase 1 Tables

**Files:**
- Modify: `src/db/models.rs`
- Modify: `src/db/queries.rs`
- Create: `src/db/queries_phase1.rs` (if queries.rs would exceed 300 lines)

- [ ] **Step 1: Add new row structs to db/models.rs**

Add after `PeerScoreRow`:

```rust
/// A round's state within a debate.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoundRow {
    pub debate_id: String,
    pub round_number: i64,
    pub status: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// An analysis result (challenge validation, divergence, pairing).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AnalysisRow {
    pub id: String,
    pub debate_id: String,
    pub bot_id: Option<String>,
    pub analysis_type: String,
    pub input_json: String,
    pub result_json: String,
    pub model_used: String,
    pub created_at: String,
}

/// Cross-examination pairing for Round 3.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PairingRow {
    pub debate_id: String,
    pub bot_a_id: String,
    pub bot_b_id: String,
    pub third_id: Option<String>,
    pub pairing_json: String,
}

/// Final synthesis output.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SynthesisRow {
    pub debate_id: String,
    pub output_json: String,
    pub model_used: String,
    pub prompt_hash: String,
    pub created_at: String,
}

/// Role rotation history entry.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoleHistoryRow {
    pub bot_id: String,
    pub debate_id: String,
    pub role: String,
}

/// Extended debate_bots row with role column.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DebateBotWithRoleRow {
    pub debate_id: String,
    pub bot_id: String,
    pub pseudonym: String,
    pub role: Option<String>,
}
```

- [ ] **Step 2: Create queries_phase1.rs with Phase 1 query functions**

Create `src/db/queries_phase1.rs`:

```rust
use sqlx::SqlitePool;
use crate::db::models::*;

/// Insert a round state record.
pub async fn insert_round(
    pool: &SqlitePool, debate_id: &str, round_number: i64, status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO rounds (debate_id, round_number, status) VALUES (?, ?, ?)"
    ).bind(debate_id).bind(round_number).bind(status)
    .execute(pool).await?;
    Ok(())
}

/// Get a specific round's state.
pub async fn get_round(
    pool: &SqlitePool, debate_id: &str, round_number: i64,
) -> Result<Option<RoundRow>, sqlx::Error> {
    sqlx::query_as::<_, RoundRow>(
        "SELECT * FROM rounds WHERE debate_id = ? AND round_number = ?"
    ).bind(debate_id).bind(round_number).fetch_optional(pool).await
}

/// Update a round's status and optionally set started_at or completed_at.
pub async fn update_round_status(
    pool: &SqlitePool, debate_id: &str, round_number: i64, status: &str,
) -> Result<(), sqlx::Error> {
    let now = chrono::Utc::now().to_rfc3339();
    match status {
        "in_progress" => {
            sqlx::query("UPDATE rounds SET status = ?, started_at = ? WHERE debate_id = ? AND round_number = ?")
                .bind(status).bind(&now).bind(debate_id).bind(round_number)
                .execute(pool).await?;
        }
        "complete" | "failed" => {
            sqlx::query("UPDATE rounds SET status = ?, completed_at = ? WHERE debate_id = ? AND round_number = ?")
                .bind(status).bind(&now).bind(debate_id).bind(round_number)
                .execute(pool).await?;
        }
        _ => {
            sqlx::query("UPDATE rounds SET status = ? WHERE debate_id = ? AND round_number = ?")
                .bind(status).bind(debate_id).bind(round_number)
                .execute(pool).await?;
        }
    }
    Ok(())
}

/// Get all rounds for a debate, ordered by round number.
pub async fn get_rounds(
    pool: &SqlitePool, debate_id: &str,
) -> Result<Vec<RoundRow>, sqlx::Error> {
    sqlx::query_as::<_, RoundRow>(
        "SELECT * FROM rounds WHERE debate_id = ? ORDER BY round_number"
    ).bind(debate_id).fetch_all(pool).await
}

/// Update a debate_bot's role assignment.
pub async fn update_debate_bot_role(
    pool: &SqlitePool, debate_id: &str, bot_id: &str, role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE debate_bots SET role = ? WHERE debate_id = ? AND bot_id = ?")
        .bind(role).bind(debate_id).bind(bot_id)
        .execute(pool).await?;
    Ok(())
}

/// Get debate bots with role information.
pub async fn get_debate_bots_with_roles(
    pool: &SqlitePool, debate_id: &str,
) -> Result<Vec<DebateBotWithRoleRow>, sqlx::Error> {
    sqlx::query_as::<_, DebateBotWithRoleRow>(
        "SELECT debate_id, bot_id, pseudonym, role FROM debate_bots WHERE debate_id = ?"
    ).bind(debate_id).fetch_all(pool).await
}

/// Insert a role history entry for rotation tracking.
pub async fn insert_role_history(
    pool: &SqlitePool, bot_id: &str, debate_id: &str, role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO role_history (bot_id, debate_id, role) VALUES (?, ?, ?)")
        .bind(bot_id).bind(debate_id).bind(role)
        .execute(pool).await?;
    Ok(())
}

/// Get the most recent role for a bot (for rotation constraint).
pub async fn get_last_role(
    pool: &SqlitePool, bot_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row: Option<RoleHistoryRow> = sqlx::query_as::<_, RoleHistoryRow>(
        "SELECT rh.* FROM role_history rh JOIN debates d ON rh.debate_id = d.id WHERE rh.bot_id = ? ORDER BY d.created_at DESC LIMIT 1"
    ).bind(bot_id).fetch_optional(pool).await?;
    Ok(row.map(|r| r.role))
}

/// Insert an analysis result.
pub async fn insert_analysis(
    pool: &SqlitePool, id: &str, debate_id: &str, bot_id: Option<&str>,
    analysis_type: &str, input_json: &str, result_json: &str, model_used: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO analyses (id, debate_id, bot_id, analysis_type, input_json, result_json, model_used) VALUES (?, ?, ?, ?, ?, ?, ?)"
    ).bind(id).bind(debate_id).bind(bot_id).bind(analysis_type)
    .bind(input_json).bind(result_json).bind(model_used)
    .execute(pool).await?;
    Ok(())
}

/// Get analyses for a debate, optionally filtered by type.
pub async fn get_analyses(
    pool: &SqlitePool, debate_id: &str, analysis_type: Option<&str>,
) -> Result<Vec<AnalysisRow>, sqlx::Error> {
    match analysis_type {
        Some(t) => {
            sqlx::query_as::<_, AnalysisRow>(
                "SELECT * FROM analyses WHERE debate_id = ? AND analysis_type = ? ORDER BY created_at"
            ).bind(debate_id).bind(t).fetch_all(pool).await
        }
        None => {
            sqlx::query_as::<_, AnalysisRow>(
                "SELECT * FROM analyses WHERE debate_id = ? ORDER BY created_at"
            ).bind(debate_id).fetch_all(pool).await
        }
    }
}

/// Insert a cross-examination pairing.
pub async fn insert_pairing(
    pool: &SqlitePool, debate_id: &str, bot_a_id: &str, bot_b_id: &str,
    third_id: Option<&str>, pairing_json: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO pairings (debate_id, bot_a_id, bot_b_id, third_id, pairing_json) VALUES (?, ?, ?, ?, ?)"
    ).bind(debate_id).bind(bot_a_id).bind(bot_b_id).bind(third_id).bind(pairing_json)
    .execute(pool).await?;
    Ok(())
}

/// Get pairings for a debate.
pub async fn get_pairings(
    pool: &SqlitePool, debate_id: &str,
) -> Result<Vec<PairingRow>, sqlx::Error> {
    sqlx::query_as::<_, PairingRow>("SELECT * FROM pairings WHERE debate_id = ?")
        .bind(debate_id).fetch_all(pool).await
}

/// Insert the final synthesis output.
pub async fn insert_synthesis(
    pool: &SqlitePool, debate_id: &str, output_json: &str,
    model_used: &str, prompt_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO syntheses (debate_id, output_json, model_used, prompt_hash) VALUES (?, ?, ?, ?)"
    ).bind(debate_id).bind(output_json).bind(model_used).bind(prompt_hash)
    .execute(pool).await?;
    Ok(())
}

/// Get the synthesis for a debate, if it exists.
pub async fn get_synthesis(
    pool: &SqlitePool, debate_id: &str,
) -> Result<Option<SynthesisRow>, sqlx::Error> {
    sqlx::query_as::<_, SynthesisRow>("SELECT * FROM syntheses WHERE debate_id = ?")
        .bind(debate_id).fetch_optional(pool).await
}

/// Insert a response with Phase 1 fields (confidence, challenge, position_change, validity).
pub async fn insert_response_full(
    pool: &SqlitePool, id: &str, debate_id: &str, round_number: i64,
    bot_id: &str, response_json: &str, confidence: Option<i64>,
    challenge_json: Option<&str>, position_change_json: Option<&str>,
    valid: bool, retry_count: i64, abstained: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO responses (id, debate_id, round_number, bot_id, response_json, confidence, challenge_json, position_change_json, valid, retry_count, abstained) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(id).bind(debate_id).bind(round_number).bind(bot_id)
    .bind(response_json).bind(confidence).bind(challenge_json)
    .bind(position_change_json).bind(valid).bind(retry_count).bind(abstained)
    .execute(pool).await?;
    Ok(())
}

/// Get all responses for a debate across all rounds, ordered by round then creation time.
pub async fn get_all_responses(
    pool: &SqlitePool, debate_id: &str,
) -> Result<Vec<ResponseRow>, sqlx::Error> {
    sqlx::query_as::<_, ResponseRow>(
        "SELECT * FROM responses WHERE debate_id = ? ORDER BY round_number, created_at"
    ).bind(debate_id).fetch_all(pool).await
}
```

- [ ] **Step 3: Update db/mod.rs to re-export the new module**

Add to `src/db/mod.rs`:

```rust
pub mod queries_phase1;
```

- [ ] **Step 4: Update ResponseRow in models.rs to include Phase 1 columns**

Replace the existing `ResponseRow` struct:

```rust
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ResponseRow {
    pub id: String,
    pub debate_id: String,
    pub round_number: i64,
    pub bot_id: String,
    pub response_json: String,
    pub confidence: Option<i64>,
    pub challenge_json: Option<String>,
    pub position_change_json: Option<String>,
    pub valid: bool,
    pub retry_count: i64,
    pub abstained: bool,
    pub created_at: String,
}
```

- [ ] **Step 5: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

Expected: all tests pass. The ResponseRow change is backward-compatible because the new columns have defaults.

- [ ] **Step 6: Commit**

```bash
git add src/db/
git commit -m "db: add Phase 1 models and queries — rounds, analyses, pairings, syntheses, role_history"
```

---

## Task 5: Add `rand` Dependency

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add rand to Cargo.toml**

Add to `[dependencies]`:

```toml
rand = "0.9"
```

- [ ] **Step 2: Sync and verify compilation**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 Cargo.toml james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add rand 0.9 for role shuffling"
```

---

## Task 6: Role Assignment with Rotation

**Files:**
- Create: `src/orchestrator/roles.rs`
- Modify: `src/orchestrator/mod.rs` (add `pub mod roles;`)

- [ ] **Step 1: Create roles.rs**

Create `src/orchestrator/roles.rs`:

```rust
use crate::types::Role;
use rand::seq::SliceRandom;
use sqlx::SqlitePool;
use crate::db::queries_phase1;

/// Assign roles to bots for a debate, respecting the rotation constraint:
/// no bot gets the same role in consecutive debates.
///
/// Returns a Vec of (bot_id, Role) pairs. Falls back to random if constraint
/// cannot be satisfied (e.g., first debate or all roles conflict).
pub async fn assign_roles(
    pool: &SqlitePool,
    bot_ids: &[String],
) -> Result<Vec<(String, Role)>, String> {
    if bot_ids.len() > 5 {
        return Err("maximum 5 bots per debate".into());
    }

    // Fetch each bot's last role
    let mut last_roles: Vec<(String, Option<Role>)> = Vec::new();
    for bot_id in bot_ids {
        let last = queries_phase1::get_last_role(pool, bot_id)
            .await
            .map_err(|e| format!("db error fetching last role: {e}"))?;
        let role = last.and_then(|s| Role::from_str(&s));
        last_roles.push((bot_id.clone(), role));
    }

    // Try up to 100 shuffles to find one that avoids consecutive same-role
    let mut roles: Vec<Role> = Role::ALL[..bot_ids.len()].to_vec();
    let mut rng = rand::rng();

    for _ in 0..100 {
        roles.shuffle(&mut rng);
        let conflict = last_roles.iter().zip(roles.iter()).any(|((_, last), assigned)| {
            last.as_ref() == Some(assigned)
        });
        if !conflict {
            return Ok(bot_ids.iter().cloned().zip(roles.into_iter()).collect());
        }
    }

    // Fallback: accept whatever shuffle we have (first debate or pathological case)
    tracing::warn!("role rotation constraint could not be satisfied after 100 attempts, using best-effort");
    roles.shuffle(&mut rng);
    Ok(bot_ids.iter().cloned().zip(roles.into_iter()).collect())
}

/// Persist role assignments to debate_bots and role_history.
pub async fn persist_role_assignments(
    pool: &SqlitePool,
    debate_id: &str,
    assignments: &[(String, Role)],
) -> Result<(), String> {
    for (bot_id, role) in assignments {
        queries_phase1::update_debate_bot_role(pool, debate_id, bot_id, role.as_str())
            .await
            .map_err(|e| format!("db error updating debate_bot role: {e}"))?;
        queries_phase1::insert_role_history(pool, bot_id, debate_id, role.as_str())
            .await
            .map_err(|e| format!("db error inserting role history: {e}"))?;
    }
    Ok(())
}
```

- [ ] **Step 2: Add module declaration to orchestrator/mod.rs**

Add at the top of `src/orchestrator/mod.rs`:

```rust
pub mod roles;
```

- [ ] **Step 3: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 4: Commit**

```bash
git add src/orchestrator/roles.rs src/orchestrator/mod.rs
git commit -m "feat: role assignment with rotation constraint"
```

---

## Task 7: Prompt Templates

**Files:**
- Create: `src/orchestrator/prompts.rs`
- Modify: `src/orchestrator/mod.rs` (add `pub mod prompts;`)

- [ ] **Step 1: Create prompts.rs with all round prompt templates**

Create `src/orchestrator/prompts.rs`:

```rust
use crate::types::Role;

/// Round 0: Blind formation prompt. Bot receives topic and role, no context.
pub fn round0_prompt(topic: &str, role: Role) -> String {
    format!(
        "You are participating in a structured adversarial debate.\n\
         Topic: {topic}\n\
         Your role: {} — {}\n\n\
         State your initial position on this topic. Be substantive and specific.\n\
         Do not hedge or equivocate — commit to a clear position consistent with your assigned role.",
        role.as_str(),
        role.description()
    )
}

/// Round 1: Anonymous distribution prompt. Bot sees all Round 0 positions.
pub fn round1_prompt(own_pseudonym: &str) -> String {
    format!(
        "Here are the initial positions from all participants (anonymised).\n\
         Your previous position was submitted as {own_pseudonym}.\n\n\
         Review all positions. You must:\n\
         1. Identify the single strongest argument that opposes your position and explain why it is strong.\n\
         2. State specifically what evidence or reasoning would cause you to change your position.\n\n\
         Do not agree with other positions unless you can articulate exactly why the argument compels agreement."
    )
}

/// Round 2: Structured rebuttal prompt. Mandatory challenge field.
pub fn round2_prompt() -> String {
    "Here are the Round 1 responses from all participants.\n\n\
     You must raise at least one specific challenge. Your challenge must:\n\
     - Target a specific claim made by another participant (cite the pseudonym and claim)\n\
     - Provide counter-evidence or identify a logical flaw\n\
     - Be classified as factual, logical, or premise-based\n\n\
     A response without an explicit challenge will be rejected.\n\n\
     Your response JSON must include a `challenge` object with fields:\n\
     - `claim_targeted`: the specific claim you are challenging\n\
     - `counter_evidence`: your counter-evidence or logical objection\n\
     - `type`: one of \"factual\", \"logical\", or \"premise\"".to_string()
}

/// Round 2: Re-prompt after failed challenge validation.
pub fn round2_reprompt(reason: &str) -> String {
    format!(
        "Your response was rejected: {reason}\n\n\
         You must raise at least one factual or logical objection to another participant's position. \
         Include a `challenge` object with `claim_targeted`, `counter_evidence`, and `type` fields. Resubmit."
    )
}

/// Round 3: Cross-examination prompt for a paired bot.
pub fn round3_question_prompt(partner_pseudonym: &str, partner_round2_response: &str) -> String {
    format!(
        "You are in cross-examination with {partner_pseudonym}.\n\
         Their position: {partner_round2_response}\n\n\
         Pose one pointed question to {partner_pseudonym} that surfaces a hidden assumption \
         or unstated dependency in their argument.\n\n\
         Be direct. Do not soften your question to avoid conflict."
    )
}

/// Round 3: Cross-examination answer prompt (pass B).
pub fn round3_answer_prompt(
    partner_pseudonym: &str,
    partner_round2_response: &str,
    question_posed_to_you: &str,
) -> String {
    format!(
        "You are in cross-examination with {partner_pseudonym}.\n\
         Their position: {partner_round2_response}\n\n\
         Answer the question posed to you by {partner_pseudonym}: \"{question_posed_to_you}\"\n\n\
         Be direct and substantive."
    )
}

/// Round 4: Final position prompt.
pub fn round4_prompt(topic: &str) -> String {
    format!(
        "This is the final round. State your final position on: {topic}\n\n\
         You must include:\n\
         1. Your final position — clear, specific, and substantive.\n\
         2. A confidence score (0-100) reflecting your genuine certainty.\n\
         3. A position_change declaration: did your position change from Round 0? \
         If yes, state what changed, what it changed from, and the specific argument that caused the change. \
         If no, state why the opposing arguments were insufficient.\n\n\
         Do not soften your position for the sake of agreement. \
         Minority positions are preserved and valued in the synthesis.\n\n\
         Your response JSON must include a `position_change` object with fields:\n\
         - `changed`: boolean\n\
         - `from_summary`: your Round 0 position (brief)\n\
         - `to_summary`: your final position (brief)\n\
         - `reason`: what caused the change (or why you didn't change)"
    )
}
```

- [ ] **Step 2: Add module declaration to orchestrator/mod.rs**

Add to `src/orchestrator/mod.rs`:

```rust
pub mod prompts;
```

- [ ] **Step 3: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 4: Commit**

```bash
git add src/orchestrator/prompts.rs src/orchestrator/mod.rs
git commit -m "feat: prompt templates for all 5 debate rounds"
```

---

## Task 8: Bot Client — Phase 1 Request/Response Types

**Files:**
- Modify: `src/bot_client/mod.rs`

- [ ] **Step 1: Add Phase 1 DebateRequest and DebateResponse types**

Add after the existing types in `src/bot_client/mod.rs`:

```rust
/// Context entry sent to bots in Rounds 1+. Contains anonymised prior responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundContext {
    pub pseudonym: String,
    pub round: i64,
    pub response: String,
    pub confidence: Option<i64>,
}

/// Phase 1 request payload for all rounds. Superset of Phase 0 PositionRequest.
#[derive(Debug, Serialize)]
pub struct DebateRoundRequest {
    pub session_id: String,
    pub round: i64,
    pub role: String,
    pub context: Vec<RoundContext>,
    pub prompt: String,
}

/// Structured challenge object (Round 2 required, optional other rounds).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeField {
    pub claim_targeted: String,
    pub counter_evidence: String,
    #[serde(rename = "type")]
    pub challenge_type: String,
}

/// Position change declaration (Round 4 required).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionChangeField {
    pub changed: bool,
    pub from_summary: String,
    pub to_summary: String,
    pub reason: String,
}

/// Phase 1 response from a bot. All fields after `response` are optional
/// depending on the round.
#[derive(Debug, Clone, Deserialize)]
pub struct DebateRoundResponse {
    pub response: String,
    pub confidence: Option<i64>,
    pub challenge: Option<ChallengeField>,
    pub position_change: Option<PositionChangeField>,
}

/// Send a Phase 1 debate round request to a bot.
pub async fn send_debate_request(
    client: &ClientWithMiddleware,
    endpoint_url: &str,
    token: &str,
    request: &DebateRoundRequest,
) -> Result<DebateRoundResponse, String> {
    let resp = client
        .post(endpoint_url)
        .bearer_auth(token)
        .json(request)
        .send()
        .await
        .map_err(|e| format!("connection failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("bot returned HTTP {}", resp.status()));
    }
    resp.json::<DebateRoundResponse>()
        .await
        .map_err(|e| format!("invalid response body: {e}"))
}
```

- [ ] **Step 2: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 3: Commit**

```bash
git add src/bot_client/mod.rs
git commit -m "feat: Phase 1 bot client types — DebateRoundRequest/Response with challenge and position_change"
```

---

## Task 9: Error Types — Analyser and Synthesis Variants

**Files:**
- Modify: `src/error.rs`

- [ ] **Step 1: Add new error variants**

Add to the `AppError` enum in `src/error.rs`:

```rust
#[error("analysis failed: {0}")]
AnalysisFailed(String),

#[error("synthesis failed: {0}")]
SynthesisFailed(String),

#[error("quorum lost: {0}")]
QuorumLost(String),

#[error("validation failed: {0}")]
ValidationFailed(String),
```

Update the `IntoResponse` match:

```rust
AppError::AnalysisFailed(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
AppError::SynthesisFailed(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
AppError::QuorumLost(msg) => (StatusCode::CONFLICT, msg.clone()),
AppError::ValidationFailed(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
```

- [ ] **Step 2: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 3: Commit**

```bash
git add src/error.rs
git commit -m "error: add AnalysisFailed, SynthesisFailed, QuorumLost, ValidationFailed variants"
```

---

## Task 10: Analyser Module — LLM Client and MiniMax Integration

**Files:**
- Create: `src/analyser/mod.rs`
- Modify: `src/lib.rs` (add `pub mod analyser;`)

The analyser module calls MiniMax for structured analysis tasks. It uses `reqwest` directly (not the middleware client, which is configured for bot retries). MiniMax is called for:
1. Challenge validation (Round 2)
2. Divergence pairing (Round 3)
3. Per-bot divergence analysis (post-Round 4)

- [ ] **Step 1: Create analyser/mod.rs with MiniMax client**

Create `src/analyser/mod.rs`:

```rust
pub mod challenge;
pub mod divergence;
pub mod pairing;

use serde::{Deserialize, Serialize};
use crate::config::ModelsConfig;

/// A generic MiniMax chat completion request.
#[derive(Debug, Serialize)]
struct MiniMaxRequest {
    model: String,
    messages: Vec<MiniMaxMessage>,
    temperature: f64,
    response_format: Option<ResponseFormat>,
}

#[derive(Debug, Serialize)]
struct MiniMaxMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(Debug, Deserialize)]
struct MiniMaxResponse {
    choices: Vec<MiniMaxChoice>,
}

#[derive(Debug, Deserialize)]
struct MiniMaxChoice {
    message: MiniMaxChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct MiniMaxChoiceMessage {
    content: String,
}

/// Call MiniMax with a system prompt and expect a JSON response string.
pub async fn call_minimax(
    config: &ModelsConfig,
    system_prompt: &str,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/chat/completions", config.minimax_base_url);

    let request = MiniMaxRequest {
        model: config.minimax_model.clone(),
        messages: vec![
            MiniMaxMessage { role: "user".into(), content: system_prompt.into() },
        ],
        temperature: 0.0,
        response_format: Some(ResponseFormat { format_type: "json_object".into() }),
    };

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.minimax_api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("MiniMax request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("MiniMax returned HTTP {status}: {body}"));
    }

    let parsed: MiniMaxResponse = resp.json()
        .await
        .map_err(|e| format!("MiniMax response parse failed: {e}"))?;

    parsed.choices.first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| "MiniMax returned empty choices".into())
}
```

- [ ] **Step 2: Add module declaration to lib.rs**

Add to `src/lib.rs`:

```rust
pub mod analyser;
```

- [ ] **Step 3: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 4: Commit**

```bash
git add src/analyser/ src/lib.rs
git commit -m "feat: analyser module with MiniMax client for structured LLM calls"
```

---

## Task 11: Challenge Validation (Round 2)

**Files:**
- Create: `src/analyser/challenge.rs`

- [ ] **Step 1: Create challenge.rs**

Create `src/analyser/challenge.rs`:

```rust
use serde::Deserialize;
use crate::analyser::call_minimax;
use crate::config::ModelsConfig;

/// Result of MiniMax challenge validation.
#[derive(Debug, Deserialize)]
pub struct ChallengeValidation {
    pub valid: bool,
    pub reason: String,
}

/// Validate a structured challenge using MiniMax.
/// Returns whether the challenge is substantive (not a vacuous restatement).
pub async fn validate_challenge(
    config: &ModelsConfig,
    challenge_json: &str,
    round2_response: &str,
) -> Result<ChallengeValidation, String> {
    let prompt = format!(
        "Does the following challenge contain a specific factual claim, logical objection, \
         or premise critique directed at a named claim from another participant? \
         Return JSON: {{ \"valid\": bool, \"reason\": \"string\" }}\n\n\
         Challenge: {challenge_json}\n\
         Context: {round2_response}"
    );

    let result = call_minimax(config, &prompt).await?;
    serde_json::from_str::<ChallengeValidation>(&result)
        .map_err(|e| format!("failed to parse challenge validation: {e}, raw: {result}"))
}
```

- [ ] **Step 2: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 3: Commit**

```bash
git add src/analyser/challenge.rs
git commit -m "feat: Round 2 challenge validation via MiniMax"
```

---

## Task 12: Divergence Pairing (Round 3)

**Files:**
- Create: `src/analyser/pairing.rs`

- [ ] **Step 1: Create pairing.rs**

Create `src/analyser/pairing.rs`:

```rust
use serde::Deserialize;
use crate::analyser::call_minimax;
use crate::config::ModelsConfig;

/// MiniMax pairing result — which bots to pair for cross-examination.
#[derive(Debug, Deserialize)]
pub struct PairingResult {
    pub pair_1: Vec<String>,
    pub pair_2: Vec<String>,
    pub third_joins: String,
    pub third: String,
}

/// Determine cross-examination pairings based on maximum semantic divergence.
/// Sends all Round 2 positions to MiniMax and gets back optimal pairings.
pub async fn compute_pairings(
    config: &ModelsConfig,
    positions: &[(String, String)], // (pseudonym, round2_response)
) -> Result<PairingResult, String> {
    let positions_text: String = positions.iter()
        .map(|(pseudo, resp)| format!("{pseudo}: {resp}"))
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt = format!(
        "Given these {} debate positions, identify the two pairs of participants whose positions \
         are most divergent. The remaining participant joins whichever pair has the most similar \
         positions (creating a 3-way). Return JSON: \
         {{ \"pair_1\": [\"Agent X\", \"Agent Y\"], \"pair_2\": [\"Agent W\", \"Agent Z\"], \
         \"third_joins\": \"pair_1\" or \"pair_2\", \"third\": \"Agent V\" }}\n\n\
         Positions:\n{positions_text}",
        positions.len()
    );

    let result = call_minimax(config, &prompt).await?;
    serde_json::from_str::<PairingResult>(&result)
        .map_err(|e| format!("failed to parse pairing result: {e}, raw: {result}"))
}
```

- [ ] **Step 2: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 3: Commit**

```bash
git add src/analyser/pairing.rs
git commit -m "feat: Round 3 cross-examination pairing via MiniMax"
```

---

## Task 13: Divergence Analysis (Post-Round 4)

**Files:**
- Create: `src/analyser/divergence.rs`

- [ ] **Step 1: Create divergence.rs**

Create `src/analyser/divergence.rs`:

```rust
use serde::Deserialize;
use crate::analyser::call_minimax;
use crate::config::ModelsConfig;

/// Per-bot divergence analysis result.
#[derive(Debug, Clone, Deserialize)]
pub struct DivergenceResult {
    pub shifted: bool,
    pub magnitude: String,
    pub what_changed: String,
    pub justification_adequate: bool,
    pub flags: Vec<String>,
}

/// Compare a bot's Round 0 and Round 4 positions using MiniMax.
pub async fn analyse_divergence(
    config: &ModelsConfig,
    round0_response: &str,
    round4_response: &str,
    position_change_json: &str,
) -> Result<DivergenceResult, String> {
    let prompt = format!(
        "Compare these two positions from the same participant in a structured debate.\n\n\
         Round 0 position: {round0_response}\n\
         Round 4 position: {round4_response}\n\
         Participant's self-declared position_change: {position_change_json}\n\n\
         Assess:\n\
         1. Did the position substantively shift? (not just rephrasing)\n\
         2. Magnitude: none | minor | major | reversal\n\
         3. What specifically changed?\n\
         4. Is the participant's self-declared justification adequate — does it cite a specific argument from the debate that accounts for the shift?\n\
         5. Any flags (e.g., shift without justification, claimed no change but position clearly different)\n\n\
         Return JSON: {{ \"shifted\": bool, \"magnitude\": \"string\", \"what_changed\": \"string\", \
         \"justification_adequate\": bool, \"flags\": [\"string\"] }}"
    );

    let result = call_minimax(config, &prompt).await?;
    serde_json::from_str::<DivergenceResult>(&result)
        .map_err(|e| format!("failed to parse divergence result: {e}, raw: {result}"))
}
```

- [ ] **Step 2: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 3: Commit**

```bash
git add src/analyser/divergence.rs
git commit -m "feat: post-Round 4 divergence analysis via MiniMax"
```

---

## Task 14: Orchestrator State Machine

**Files:**
- Create: `src/orchestrator/state_machine.rs`
- Modify: `src/orchestrator/mod.rs` (add `pub mod state_machine;`)

- [ ] **Step 1: Create state_machine.rs**

Create `src/orchestrator/state_machine.rs`:

```rust
use sqlx::SqlitePool;
use crate::db::queries_phase1;

/// Initialise round records for a new multi-round debate (Rounds 0-4).
pub async fn init_rounds(
    pool: &SqlitePool,
    debate_id: &str,
) -> Result<(), String> {
    for round in 0..=4 {
        queries_phase1::insert_round(pool, debate_id, round, "pending")
            .await
            .map_err(|e| format!("failed to init round {round}: {e}"))?;
    }
    Ok(())
}

/// Mark a round as in-progress.
pub async fn start_round(
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
) -> Result<(), String> {
    queries_phase1::update_round_status(pool, debate_id, round_number, "in_progress")
        .await
        .map_err(|e| format!("failed to start round {round_number}: {e}"))
}

/// Mark a round as complete.
pub async fn complete_round(
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
) -> Result<(), String> {
    queries_phase1::update_round_status(pool, debate_id, round_number, "complete")
        .await
        .map_err(|e| format!("failed to complete round {round_number}: {e}"))
}

/// Mark a round as failed.
pub async fn fail_round(
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
) -> Result<(), String> {
    queries_phase1::update_round_status(pool, debate_id, round_number, "failed")
        .await
        .map_err(|e| format!("failed to fail round {round_number}: {e}"))
}

/// Find the next round to run for resumption. Returns the first round
/// that is not yet complete. Returns None if all rounds are complete.
pub async fn find_resume_point(
    pool: &SqlitePool,
    debate_id: &str,
) -> Result<Option<i64>, String> {
    let rounds = queries_phase1::get_rounds(pool, debate_id)
        .await
        .map_err(|e| format!("failed to get rounds: {e}"))?;

    for round in rounds {
        if round.status != "complete" {
            return Ok(Some(round.round_number));
        }
    }
    Ok(None)
}
```

- [ ] **Step 2: Add module declaration**

Add to `src/orchestrator/mod.rs`:

```rust
pub mod state_machine;
```

- [ ] **Step 3: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 4: Commit**

```bash
git add src/orchestrator/state_machine.rs src/orchestrator/mod.rs
git commit -m "feat: round state machine with init, transitions, and resumption"
```

---

## Task 15: Round Implementations — Rounds 0-4

**Files:**
- Create: `src/orchestrator/rounds/mod.rs`
- Create: `src/orchestrator/rounds/round0.rs`
- Create: `src/orchestrator/rounds/round1.rs`
- Create: `src/orchestrator/rounds/round2.rs`
- Create: `src/orchestrator/rounds/round3.rs`
- Create: `src/orchestrator/rounds/round4.rs`
- Modify: `src/orchestrator/mod.rs` (add `pub mod rounds;`)

This is the largest task. Each round is a separate file with a single `pub async fn run_roundN(...)` function.

- [ ] **Step 1: Create rounds/mod.rs**

Create `src/orchestrator/rounds/mod.rs`:

```rust
pub mod round0;
pub mod round1;
pub mod round2;
pub mod round3;
pub mod round4;
```

- [ ] **Step 2: Create rounds/round0.rs — Blind Formation**

Create `src/orchestrator/rounds/round0.rs`:

```rust
use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{self, DebateRoundRequest, RoundContext, DebateRoundResponse};
use crate::db::{models::BotRow, queries};
use crate::db::queries_phase1;
use crate::orchestrator::prompts;
use crate::types::Role;

/// Dispatch topic to all bots concurrently. No context. Role assigned.
/// Returns (bot_id, Option<DebateRoundResponse>) pairs.
pub async fn run_round0(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    timeout_secs: u64,
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    let futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = debate_id.to_string();
        let role = role_assignments.get(&bot.id).copied().unwrap_or(Role::Proponent);
        let prompt = prompts::round0_prompt(topic, role);
        let bot_id = bot.id.clone();
        async move {
            let req = DebateRoundRequest {
                session_id,
                round: 0,
                role: role.as_str().to_string(),
                context: vec![],
                prompt,
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                bot_client::send_debate_request(&client, &endpoint, &token, &req),
            ).await;
            match result {
                Ok(Ok(resp)) => (bot_id, Some(resp)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot_id, error = %e, "Round 0: bot request failed");
                    (bot_id, None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot_id, "Round 0: bot request timed out");
                    (bot_id, None)
                }
            }
        }
    }).collect();

    let results = futures::future::join_all(futures).await;

    // Store responses
    let debate_bots = queries_phase1::get_debate_bots_with_roles(pool, debate_id)
        .await.map_err(|e| format!("db error: {e}"))?;

    for (bot_id, resp_opt) in &results {
        let (response_text, abstained) = match resp_opt {
            Some(r) => (r.response.clone(), false),
            None => ("(abstained)".to_string(), true),
        };
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool, &resp_id, debate_id, 0, bot_id, &response_text,
            None, None, None, true, 0, abstained,
        ).await.map_err(|e| format!("db error storing Round 0 response: {e}"))?;
    }

    Ok(results)
}
```

- [ ] **Step 3: Create rounds/round1.rs — Anonymous Distribution**

Create `src/orchestrator/rounds/round1.rs`:

```rust
use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{self, DebateRoundRequest, RoundContext, DebateRoundResponse};
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::orchestrator::prompts;
use crate::types::Role;

/// Build anonymised context from Round 0 responses (all bots, including own).
fn build_round0_context(
    round0_results: &[(String, Option<DebateRoundResponse>)],
    pseudonym_map: &HashMap<String, String>, // bot_id -> pseudonym
) -> Vec<RoundContext> {
    round0_results.iter()
        .filter_map(|(bot_id, resp_opt)| {
            resp_opt.as_ref().map(|r| {
                let pseudonym = pseudonym_map.get(bot_id).cloned().unwrap_or_else(|| "Unknown".into());
                RoundContext {
                    pseudonym,
                    round: 0,
                    response: r.response.clone(),
                    confidence: None,
                }
            })
        })
        .collect()
}

/// Dispatch Round 1 to all bots with anonymised Round 0 context.
pub async fn run_round1(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    pseudonym_map: &HashMap<String, String>,
    round0_context: Vec<RoundContext>,
    timeout_secs: u64,
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    let futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = debate_id.to_string();
        let role = role_assignments.get(&bot.id).copied().unwrap_or(Role::Proponent);
        let own_pseudonym = pseudonym_map.get(&bot.id).cloned().unwrap_or_default();
        let prompt = prompts::round1_prompt(&own_pseudonym);
        let context = round0_context.clone();
        let bot_id = bot.id.clone();
        async move {
            let req = DebateRoundRequest {
                session_id,
                round: 1,
                role: role.as_str().to_string(),
                context,
                prompt,
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                bot_client::send_debate_request(&client, &endpoint, &token, &req),
            ).await;
            match result {
                Ok(Ok(resp)) => (bot_id, Some(resp)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot_id, error = %e, "Round 1: bot request failed");
                    (bot_id, None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot_id, "Round 1: bot request timed out");
                    (bot_id, None)
                }
            }
        }
    }).collect();

    let results = futures::future::join_all(futures).await;

    // Store responses
    for (bot_id, resp_opt) in &results {
        let (response_text, confidence, abstained) = match resp_opt {
            Some(r) => (r.response.clone(), r.confidence, false),
            None => ("(abstained)".to_string(), None, true),
        };
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool, &resp_id, debate_id, 1, bot_id, &response_text,
            confidence, None, None, true, 0, abstained,
        ).await.map_err(|e| format!("db error storing Round 1 response: {e}"))?;
    }

    Ok(results)
}
```

- [ ] **Step 4: Create rounds/round2.rs — Structured Rebuttal with Validation**

Create `src/orchestrator/rounds/round2.rs`:

```rust
use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{self, DebateRoundRequest, RoundContext, DebateRoundResponse};
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::analyser::challenge::validate_challenge;
use crate::config::ModelsConfig;
use crate::orchestrator::prompts;
use crate::types::Role;

/// Run Round 2: structured rebuttal with MiniMax challenge validation.
/// Bots that fail validation are re-prompted up to max_retries times.
pub async fn run_round2(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    round1_context: Vec<RoundContext>,
    models_config: &ModelsConfig,
    timeout_secs: u64,
    max_retries: u32,
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    let mut final_results: Vec<(String, Option<DebateRoundResponse>)> = Vec::new();

    // Dispatch concurrently first
    let initial_futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = debate_id.to_string();
        let role = role_assignments.get(&bot.id).copied().unwrap_or(Role::Proponent);
        let prompt = prompts::round2_prompt();
        let context = round1_context.clone();
        let bot_id = bot.id.clone();
        async move {
            let req = DebateRoundRequest {
                session_id,
                round: 2,
                role: role.as_str().to_string(),
                context,
                prompt,
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                bot_client::send_debate_request(&client, &endpoint, &token, &req),
            ).await;
            match result {
                Ok(Ok(resp)) => (bot_id, Some(resp)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot_id, error = %e, "Round 2: bot request failed");
                    (bot_id, None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot_id, "Round 2: bot request timed out");
                    (bot_id, None)
                }
            }
        }
    }).collect();

    let initial_results = futures::future::join_all(initial_futures).await;

    // Validate challenges and re-prompt if needed (sequential per bot)
    for (bot_id, resp_opt) in initial_results {
        let bot = bots.iter().find(|b| b.id == bot_id);
        let endpoint = bot.map(|b| b.endpoint_url.as_str()).unwrap_or("");
        let token = bot_tokens.get(&bot_id).cloned().unwrap_or_default();
        let role = role_assignments.get(&bot_id).copied().unwrap_or(Role::Proponent);

        match resp_opt {
            None => {
                // Abstained — store and move on
                let resp_id = uuid::Uuid::new_v4().to_string();
                queries_phase1::insert_response_full(
                    pool, &resp_id, debate_id, 2, &bot_id, "(abstained)",
                    None, None, None, true, 0, true,
                ).await.map_err(|e| format!("db error: {e}"))?;
                final_results.push((bot_id, None));
            }
            Some(mut resp) => {
                let mut retry_count: u32 = 0;
                let mut valid = false;

                loop {
                    // Check challenge field exists
                    if let Some(ref challenge) = resp.challenge {
                        let challenge_json = serde_json::to_string(challenge)
                            .unwrap_or_default();
                        // Validate with MiniMax
                        match validate_challenge(models_config, &challenge_json, &resp.response).await {
                            Ok(validation) if validation.valid => {
                                valid = true;
                                break;
                            }
                            Ok(validation) => {
                                tracing::info!(bot_id = %bot_id, reason = %validation.reason, "Round 2: challenge rejected");
                                if retry_count >= max_retries {
                                    tracing::warn!(bot_id = %bot_id, "Round 2: max retries reached, marking invalid");
                                    break;
                                }
                                // Re-prompt
                                let reprompt = prompts::round2_reprompt(&validation.reason);
                                let req = DebateRoundRequest {
                                    session_id: debate_id.to_string(),
                                    round: 2,
                                    role: role.as_str().to_string(),
                                    context: round1_context.clone(),
                                    prompt: reprompt,
                                };
                                match bot_client::send_debate_request(client, endpoint, &token, &req).await {
                                    Ok(new_resp) => {
                                        resp = new_resp;
                                        retry_count += 1;
                                    }
                                    Err(e) => {
                                        tracing::warn!(bot_id = %bot_id, error = %e, "Round 2: re-prompt failed");
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(bot_id = %bot_id, error = %e, "Round 2: MiniMax validation error, accepting as-is");
                                valid = true;
                                break;
                            }
                        }
                    } else {
                        // No challenge field at all
                        if retry_count >= max_retries {
                            tracing::warn!(bot_id = %bot_id, "Round 2: no challenge after retries, marking invalid");
                            break;
                        }
                        let reprompt = prompts::round2_reprompt("No challenge object found in response");
                        let req = DebateRoundRequest {
                            session_id: debate_id.to_string(),
                            round: 2,
                            role: role.as_str().to_string(),
                            context: round1_context.clone(),
                            prompt: reprompt,
                        };
                        match bot_client::send_debate_request(client, endpoint, &token, &req).await {
                            Ok(new_resp) => {
                                resp = new_resp;
                                retry_count += 1;
                            }
                            Err(e) => {
                                tracing::warn!(bot_id = %bot_id, error = %e, "Round 2: re-prompt failed");
                                break;
                            }
                        }
                    }
                }

                let challenge_json = resp.challenge.as_ref()
                    .and_then(|c| serde_json::to_string(c).ok());
                let resp_id = uuid::Uuid::new_v4().to_string();
                queries_phase1::insert_response_full(
                    pool, &resp_id, debate_id, 2, &bot_id, &resp.response,
                    resp.confidence, challenge_json.as_deref(), None,
                    valid, retry_count as i64, false,
                ).await.map_err(|e| format!("db error: {e}"))?;

                // Store validation analysis
                if let Some(ref challenge) = resp.challenge {
                    let analysis_id = uuid::Uuid::new_v4().to_string();
                    let input = serde_json::to_string(challenge).unwrap_or_default();
                    let result = serde_json::json!({ "valid": valid, "retry_count": retry_count });
                    let _ = queries_phase1::insert_analysis(
                        pool, &analysis_id, debate_id, Some(&bot_id),
                        "challenge_validation", &input, &result.to_string(),
                        &models_config.minimax_model,
                    ).await; // intentional: log and continue
                }

                final_results.push((bot_id, Some(resp)));
            }
        }
    }

    Ok(final_results)
}
```

- [ ] **Step 5: Create rounds/round3.rs — Cross-Examination**

Create `src/orchestrator/rounds/round3.rs`:

```rust
use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{self, DebateRoundRequest, RoundContext, DebateRoundResponse};
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::analyser::pairing::{compute_pairings, PairingResult};
use crate::config::ModelsConfig;
use crate::orchestrator::prompts;
use crate::types::Role;

/// Run Round 3: Cross-examination in two passes.
/// Pass A: each bot poses a question to its partner.
/// Pass B: each bot answers the question posed to it.
pub async fn run_round3(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    pseudonym_map: &HashMap<String, String>,       // bot_id -> pseudonym
    reverse_pseudonym_map: &HashMap<String, String>, // pseudonym -> bot_id
    round2_responses: &HashMap<String, String>,     // pseudonym -> round2 response text
    models_config: &ModelsConfig,
    timeout_secs: u64,
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    // Step 1: Compute pairings via MiniMax
    let positions: Vec<(String, String)> = round2_responses.iter()
        .map(|(p, r)| (p.clone(), r.clone()))
        .collect();
    let pairing = compute_pairings(models_config, &positions).await?;

    // Store pairing
    let resolve_bot_id = |pseudo: &str| -> String {
        reverse_pseudonym_map.get(pseudo).cloned().unwrap_or_default()
    };
    let pair1_a = resolve_bot_id(&pairing.pair_1[0]);
    let pair1_b = resolve_bot_id(&pairing.pair_1[1]);
    let pair2_a = resolve_bot_id(&pairing.pair_2[0]);
    let pair2_b = resolve_bot_id(&pairing.pair_2[1]);
    let third_bot = resolve_bot_id(&pairing.third);

    let pairing_json = serde_json::to_string(&pairing).unwrap_or_default();
    queries_phase1::insert_pairing(
        pool, debate_id, &pair1_a, &pair1_b,
        Some(&third_bot), &pairing_json,
    ).await.map_err(|e| format!("db error storing pairing: {e}"))?;

    // Build directed pairs: each bot -> its cross-exam partner
    // For pair_1 and pair_2: symmetric (A questions B, B questions A)
    // For the 3-way: round-robin (A->B, B->C, C->A)
    let mut question_targets: Vec<(String, String)> = Vec::new(); // (questioner_pseudo, target_pseudo)

    // Pair 1
    question_targets.push((pairing.pair_1[0].clone(), pairing.pair_1[1].clone()));
    question_targets.push((pairing.pair_1[1].clone(), pairing.pair_1[0].clone()));
    // Pair 2
    question_targets.push((pairing.pair_2[0].clone(), pairing.pair_2[1].clone()));
    question_targets.push((pairing.pair_2[1].clone(), pairing.pair_2[0].clone()));
    // Third joins one pair — add round-robin
    let joined_pair = if pairing.third_joins == "pair_1" { &pairing.pair_1 } else { &pairing.pair_2 };
    question_targets.push((pairing.third.clone(), joined_pair[0].clone()));
    // The partner that the third was added to now also questions the third
    question_targets.push((joined_pair[0].clone(), pairing.third.clone()));

    // Pass A: Each bot poses a question (concurrent)
    let pass_a_futures: Vec<_> = question_targets.iter().map(|(questioner_pseudo, target_pseudo)| {
        let questioner_bot_id = resolve_bot_id(questioner_pseudo);
        let bot = bots.iter().find(|b| b.id == questioner_bot_id);
        let endpoint = bot.map(|b| b.endpoint_url.clone()).unwrap_or_default();
        let token = bot_tokens.get(&questioner_bot_id).cloned().unwrap_or_default();
        let role = role_assignments.get(&questioner_bot_id).copied().unwrap_or(Role::Proponent);
        let target_response = round2_responses.get(target_pseudo.as_str()).cloned().unwrap_or_default();
        let prompt = prompts::round3_question_prompt(target_pseudo, &target_response);
        let session_id = debate_id.to_string();
        let client = client.clone();
        let questioner_id = questioner_bot_id.clone();
        async move {
            let req = DebateRoundRequest {
                session_id,
                round: 3,
                role: role.as_str().to_string(),
                context: vec![],
                prompt,
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                bot_client::send_debate_request(&client, &endpoint, &token, &req),
            ).await;
            match result {
                Ok(Ok(resp)) => (questioner_id, Some(resp.response)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %questioner_id, error = %e, "Round 3 Pass A: failed");
                    (questioner_id, None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %questioner_id, "Round 3 Pass A: timed out");
                    (questioner_id, None)
                }
            }
        }
    }).collect();

    let pass_a_results = futures::future::join_all(pass_a_futures).await;

    // Build question map: target_bot_id -> (questioner_pseudo, question_text)
    let mut questions_for: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for ((questioner_pseudo, target_pseudo), (_, question_opt)) in question_targets.iter().zip(pass_a_results.iter()) {
        if let Some(question) = question_opt {
            let target_bot_id = resolve_bot_id(target_pseudo);
            questions_for.entry(target_bot_id)
                .or_default()
                .push((questioner_pseudo.clone(), question.clone()));
        }
    }

    // Pass B: Each bot answers questions posed to it (concurrent)
    let pass_b_futures: Vec<_> = bots.iter().filter_map(|bot| {
        let questions = questions_for.get(&bot.id)?;
        let first_question = questions.first()?;
        let questioner_pseudo = first_question.0.clone();
        let question_text = first_question.1.clone();
        let partner_response = round2_responses.get(&questioner_pseudo).cloned().unwrap_or_default();
        let prompt = prompts::round3_answer_prompt(&questioner_pseudo, &partner_response, &question_text);

        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let role = role_assignments.get(&bot.id).copied().unwrap_or(Role::Proponent);
        let session_id = debate_id.to_string();
        let bot_id = bot.id.clone();
        Some(async move {
            let req = DebateRoundRequest {
                session_id,
                round: 3,
                role: role.as_str().to_string(),
                context: vec![],
                prompt,
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                bot_client::send_debate_request(&client, &endpoint, &token, &req),
            ).await;
            match result {
                Ok(Ok(resp)) => (bot_id, Some(resp)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot_id, error = %e, "Round 3 Pass B: failed");
                    (bot_id, None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot_id, "Round 3 Pass B: timed out");
                    (bot_id, None)
                }
            }
        })
    }).collect();

    let pass_b_results = futures::future::join_all(pass_b_futures).await;

    // Store all Round 3 responses (both passes combined into one response per bot)
    // We concatenate the question (Pass A) and answer (Pass B) for each bot
    let mut all_results: Vec<(String, Option<DebateRoundResponse>)> = Vec::new();

    for (bot_id, answer_opt) in &pass_b_results {
        let question_text = pass_a_results.iter()
            .find(|(id, _)| id == bot_id)
            .and_then(|(_, q)| q.clone());

        let combined_response = match (question_text, answer_opt) {
            (Some(q), Some(a)) => format!("[Question posed]: {q}\n\n[Answer given]: {}", a.response),
            (Some(q), None) => format!("[Question posed]: {q}\n\n[Answer]: (no answer)"),
            (None, Some(a)) => format!("[Question]: (none)\n\n[Answer given]: {}", a.response),
            (None, None) => "(abstained)".to_string(),
        };
        let abstained = answer_opt.is_none() && question_text.is_none();
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool, &resp_id, debate_id, 3, bot_id, &combined_response,
            answer_opt.as_ref().and_then(|r| r.confidence), None, None,
            true, 0, abstained,
        ).await.map_err(|e| format!("db error storing Round 3 response: {e}"))?;

        all_results.push((bot_id.clone(), answer_opt.clone()));
    }

    Ok(all_results)
}
```

- [ ] **Step 6: Create rounds/round4.rs — Final Position**

Create `src/orchestrator/rounds/round4.rs`:

```rust
use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{self, DebateRoundRequest, RoundContext, DebateRoundResponse};
use crate::db::models::BotRow;
use crate::db::queries_phase1;
use crate::orchestrator::prompts;
use crate::types::Role;

/// Run Round 4: Final position with position_change declaration.
pub async fn run_round4(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &str,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    role_assignments: &HashMap<String, Role>,
    full_context: Vec<RoundContext>,
    timeout_secs: u64,
) -> Result<Vec<(String, Option<DebateRoundResponse>)>, String> {
    let prompt = prompts::round4_prompt(topic);

    let futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = debate_id.to_string();
        let role = role_assignments.get(&bot.id).copied().unwrap_or(Role::Proponent);
        let prompt = prompt.clone();
        let context = full_context.clone();
        let bot_id = bot.id.clone();
        async move {
            let req = DebateRoundRequest {
                session_id,
                round: 4,
                role: role.as_str().to_string(),
                context,
                prompt,
            };
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout_secs),
                bot_client::send_debate_request(&client, &endpoint, &token, &req),
            ).await;
            match result {
                Ok(Ok(resp)) => (bot_id, Some(resp)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot_id, error = %e, "Round 4: bot request failed");
                    (bot_id, None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot_id, "Round 4: bot request timed out");
                    (bot_id, None)
                }
            }
        }
    }).collect();

    let results = futures::future::join_all(futures).await;

    // Store responses
    for (bot_id, resp_opt) in &results {
        let (response_text, confidence, position_change_json, abstained) = match resp_opt {
            Some(r) => {
                let pc_json = r.position_change.as_ref()
                    .and_then(|pc| serde_json::to_string(pc).ok());
                (r.response.clone(), r.confidence, pc_json, false)
            }
            None => ("(abstained)".to_string(), None, None, true),
        };
        let resp_id = uuid::Uuid::new_v4().to_string();
        queries_phase1::insert_response_full(
            pool, &resp_id, debate_id, 4, bot_id, &response_text,
            confidence, None, position_change_json.as_deref(),
            true, 0, abstained,
        ).await.map_err(|e| format!("db error storing Round 4 response: {e}"))?;
    }

    Ok(results)
}
```

- [ ] **Step 7: Add rounds module declaration to orchestrator/mod.rs**

Add to `src/orchestrator/mod.rs`:

```rust
pub mod rounds;
```

- [ ] **Step 8: Sync and test compilation**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 9: Commit**

```bash
git add src/orchestrator/rounds/
git commit -m "feat: Round 0-4 implementations — blind formation through final position"
```

---

## Task 16: Synthesis — Pre-Computation, Schema, and Opus Call

**Files:**
- Create: `src/synthesiser/mod.rs`
- Create: `src/synthesiser/precompute.rs`
- Create: `src/synthesiser/schema.rs`
- Modify: `src/lib.rs` (add `pub mod synthesiser;`)

- [ ] **Step 1: Create synthesiser/schema.rs — output types**

Create `src/synthesiser/schema.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The rigid output schema for Opus synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisOutput {
    pub topic: String,
    pub consensus_points: Vec<ConsensusPoint>,
    pub live_disagreements: Vec<LiveDisagreement>,
    pub flagged_capitulations: Vec<FlaggedCapitulation>,
    pub minority_positions: Vec<MinorityPosition>,
    pub confidence_trajectories: HashMap<String, Vec<Option<i64>>>,
    pub meta_observations: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusPoint {
    pub point: String,
    pub supporting_bots: Vec<String>,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDisagreement {
    pub issue: String,
    pub side_a: DisagreementSide,
    pub side_b: DisagreementSide,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisagreementSide {
    pub position: String,
    pub bots: Vec<String>,
    pub best_argument: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlaggedCapitulation {
    pub bot: String,
    pub from: String,
    pub to: String,
    pub justification_adequate: bool,
    pub flag_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinorityPosition {
    pub bot: String,
    pub position: String,
    pub key_argument: String,
    pub confidence: i64,
}
```

- [ ] **Step 2: Create synthesiser/precompute.rs — deterministic pre-computation**

Create `src/synthesiser/precompute.rs`:

```rust
use std::collections::HashMap;
use crate::db::models::ResponseRow;
use crate::bot_client::PositionChangeField;
use serde::Serialize;

/// Pre-computed structural data fed to the synthesis prompt.
#[derive(Debug, Serialize)]
pub struct PrecomputedData {
    pub confidence_trajectories: HashMap<String, Vec<Option<i64>>>,
    pub position_changes: Vec<PositionChangeSummary>,
    pub challenge_graph: Vec<ChallengeSummary>,
}

#[derive(Debug, Serialize)]
pub struct PositionChangeSummary {
    pub pseudonym: String,
    pub changed: bool,
    pub from_summary: String,
    pub to_summary: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct ChallengeSummary {
    pub challenger_pseudonym: String,
    pub claim_targeted: String,
    pub challenge_type: String,
}

/// Compute structural data from stored responses.
/// pseudonym_map: bot_id -> pseudonym
pub fn precompute(
    responses: &[ResponseRow],
    pseudonym_map: &HashMap<String, String>,
) -> PrecomputedData {
    // Confidence trajectories: per pseudonym, Round 1-4 confidence values
    let mut trajectories: HashMap<String, Vec<Option<i64>>> = HashMap::new();
    for resp in responses {
        let pseudonym = pseudonym_map.get(&resp.bot_id).cloned().unwrap_or_default();
        let entry = trajectories.entry(pseudonym).or_insert_with(|| vec![None; 5]);
        if (resp.round_number as usize) < 5 {
            entry[resp.round_number as usize] = resp.confidence;
        }
    }

    // Position changes from Round 4
    let mut position_changes = Vec::new();
    for resp in responses.iter().filter(|r| r.round_number == 4) {
        let pseudonym = pseudonym_map.get(&resp.bot_id).cloned().unwrap_or_default();
        if let Some(ref pc_json) = resp.position_change_json {
            if let Ok(pc) = serde_json::from_str::<PositionChangeField>(pc_json) {
                position_changes.push(PositionChangeSummary {
                    pseudonym,
                    changed: pc.changed,
                    from_summary: pc.from_summary,
                    to_summary: pc.to_summary,
                    reason: pc.reason,
                });
            }
        }
    }

    // Challenge graph from Round 2
    let mut challenge_graph = Vec::new();
    for resp in responses.iter().filter(|r| r.round_number == 2) {
        let pseudonym = pseudonym_map.get(&resp.bot_id).cloned().unwrap_or_default();
        if let Some(ref cj) = resp.challenge_json {
            if let Ok(c) = serde_json::from_str::<crate::bot_client::ChallengeField>(cj) {
                challenge_graph.push(ChallengeSummary {
                    challenger_pseudonym: pseudonym,
                    claim_targeted: c.claim_targeted,
                    challenge_type: c.challenge_type,
                });
            }
        }
    }

    PrecomputedData { confidence_trajectories: trajectories, position_changes, challenge_graph }
}
```

- [ ] **Step 3: Create synthesiser/mod.rs — Opus synthesis call**

Create `src/synthesiser/mod.rs`:

```rust
pub mod precompute;
pub mod schema;

use serde::{Deserialize, Serialize};
use crate::config::ModelsConfig;
use crate::analyser::divergence::DivergenceResult;
use sha2::{Sha256, Digest};

/// Call Opus to produce the final synthesis.
pub async fn run_synthesis(
    config: &ModelsConfig,
    topic: &str,
    transcript_text: &str,
    precomputed_json: &str,
    divergence_results_json: &str,
    temperature: f64,
) -> Result<(String, String), String> {
    let system_prompt = build_synthesis_prompt(topic, transcript_text, precomputed_json, divergence_results_json);
    let prompt_hash = {
        let mut hasher = Sha256::new();
        hasher.update(system_prompt.as_bytes());
        hex::encode(hasher.finalize())
    };

    let client = reqwest::Client::new();
    let request = AnthropicRequest {
        model: config.opus_model.clone(),
        max_tokens: 4096,
        temperature,
        messages: vec![
            AnthropicMessage { role: "user".into(), content: system_prompt },
        ],
    };

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &config.opus_api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("Opus request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Opus returned HTTP {status}: {body}"));
    }

    let parsed: AnthropicResponse = resp.json()
        .await
        .map_err(|e| format!("Opus response parse failed: {e}"))?;

    let content = parsed.content.first()
        .map(|c| c.text.clone())
        .ok_or_else(|| "Opus returned empty content".to_string())?;

    Ok((content, prompt_hash))
}

fn build_synthesis_prompt(
    topic: &str,
    transcript: &str,
    precomputed: &str,
    divergence: &str,
) -> String {
    format!(
        "You are the synthesis engine for a structured adversarial debate. Your role is analytical, not creative. \
         You must produce a rigorous, citation-backed synthesis.\n\n\
         RULES:\n\
         - Every factual claim must cite [Bot pseudonym, Round N].\n\
         - Do not infer what a participant \"seemed to mean\" — use only their stated positions.\n\
         - Do not declare consensus unless all participants explicitly agree on the specific point.\n\
         - Preserve minority positions with full dignity — a lone dissent with strong reasoning is more valuable than a 4-1 majority with weak reasoning.\n\
         - Flag any position shift that lacks adequate justification (from the divergence analysis).\n\n\
         TOPIC: {topic}\n\n\
         FULL TRANSCRIPT:\n{transcript}\n\n\
         PRE-COMPUTED STRUCTURAL DATA:\n{precomputed}\n\n\
         DIVERGENCE ANALYSES:\n{divergence}\n\n\
         OUTPUT SCHEMA (return valid JSON):\n\
         {{\n\
           \"topic\": \"string\",\n\
           \"consensus_points\": [{{ \"point\": \"string\", \"supporting_bots\": [\"pseudonym\"], \"evidence\": \"string [citations]\" }}],\n\
           \"live_disagreements\": [{{ \"issue\": \"string\", \"side_a\": {{ \"position\": \"string\", \"bots\": [\"pseudonym\"], \"best_argument\": \"string [citation]\" }}, \"side_b\": {{ \"position\": \"string\", \"bots\": [\"pseudonym\"], \"best_argument\": \"string [citation]\" }} }}],\n\
           \"flagged_capitulations\": [{{ \"bot\": \"pseudonym\", \"from\": \"string\", \"to\": \"string\", \"justification_adequate\": bool, \"flag_reason\": \"string\" }}],\n\
           \"minority_positions\": [{{ \"bot\": \"pseudonym\", \"position\": \"string\", \"key_argument\": \"string [citation]\", \"confidence\": int }}],\n\
           \"confidence_trajectories\": {{ \"pseudonym\": [null, int, int, int, int] }},\n\
           \"meta_observations\": \"string — max 200 words\"\n\
         }}"
    )
}

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    temperature: f64,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    text: String,
}
```

- [ ] **Step 4: Add module declaration to lib.rs**

Add to `src/lib.rs`:

```rust
pub mod synthesiser;
```

- [ ] **Step 5: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 6: Commit**

```bash
git add src/synthesiser/ src/lib.rs
git commit -m "feat: synthesis module — pre-computation, schema, Opus call"
```

---

## Task 17: Multi-Round Orchestrator — Replace run_debate

**Files:**
- Modify: `src/orchestrator/mod.rs`

This is the central driver that wires together roles, state machine, rounds, analysis, and synthesis.

- [ ] **Step 1: Rewrite orchestrator/mod.rs**

Replace the contents of `src/orchestrator/mod.rs` with:

```rust
pub mod anonymiser;
pub mod prompts;
pub mod roles;
pub mod rounds;
pub mod state_machine;

use std::collections::HashMap;
use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{self, RoundContext, DebateRoundResponse};
use crate::config::{ModelsConfig, DebateConfig};
use crate::db::{models::BotRow, queries, queries_phase1};
use crate::analyser::divergence::analyse_divergence;
use crate::synthesiser::{self, precompute};
use crate::types::{DebateId, Role};

/// Result of a completed Phase 0 debate (kept for backward compat).
pub struct DebateResult {
    pub debate_id: String,
    pub rankings: Vec<RankedEntry>,
}

/// A ranked argument with aggregated scores (Phase 0).
pub struct RankedEntry {
    pub pseudonym: String,
    pub avg_reasoning_quality: f64,
    pub avg_factual_grounding: f64,
    pub avg_overall: f64,
    pub total_scores: usize,
}

/// Run a full 5-round adversarial debate.
pub async fn run_multi_round_debate(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &DebateId,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &HashMap<String, String>,
    models_config: &ModelsConfig,
    debate_config: &DebateConfig,
) -> Result<(), String> {
    let id = debate_id.as_str();
    let timeout = debate_config.default_timeout_secs;

    // Build pseudonym map from debate_bots table
    let debate_bots = queries_phase1::get_debate_bots_with_roles(pool, id)
        .await.map_err(|e| format!("db error: {e}"))?;
    let pseudonym_map: HashMap<String, String> = debate_bots.iter()
        .map(|db| (db.bot_id.clone(), db.pseudonym.clone()))
        .collect();
    let reverse_pseudonym_map: HashMap<String, String> = debate_bots.iter()
        .map(|db| (db.pseudonym.clone(), db.bot_id.clone()))
        .collect();

    // Build role assignments from debate_bots
    let role_assignments: HashMap<String, Role> = debate_bots.iter()
        .filter_map(|db| {
            db.role.as_ref()
                .and_then(|r| Role::from_str(r))
                .map(|role| (db.bot_id.clone(), role))
        })
        .collect();

    // Resumption: find where to start
    let resume_round = state_machine::find_resume_point(pool, id).await?
        .unwrap_or(0);

    // Active (non-abstained) bots — starts as all, may shrink if quorum tracking needed
    let active_bots = bots;

    // === ROUND 0 ===
    if resume_round <= 0 {
        queries::update_debate_status(pool, id, "round_0")
            .await.map_err(|e| format!("db error: {e}"))?;
        state_machine::start_round(pool, id, 0).await?;

        let r0_results = rounds::round0::run_round0(
            pool, client, id, topic, active_bots, bot_tokens, &role_assignments, timeout,
        ).await?;

        let active_count = r0_results.iter().filter(|(_, r)| r.is_some()).count();
        if active_count < debate_config.quorum {
            state_machine::fail_round(pool, id, 0).await?;
            queries::update_debate_status(pool, id, "failed")
                .await.map_err(|e| format!("db error: {e}"))?;
            return Err(format!("Round 0 quorum not met: {} of {} required", active_count, debate_config.quorum));
        }
        state_machine::complete_round(pool, id, 0).await?;
    }

    // Build Round 0 context for subsequent rounds
    let r0_responses = queries::get_responses(pool, id, 0)
        .await.map_err(|e| format!("db error: {e}"))?;
    let round0_context: Vec<RoundContext> = r0_responses.iter()
        .filter(|r| !r.abstained)
        .map(|r| {
            let pseudo = pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default();
            RoundContext { pseudonym: pseudo, round: 0, response: r.response_json.clone(), confidence: None }
        })
        .collect();

    // === ROUND 1 ===
    if resume_round <= 1 {
        queries::update_debate_status(pool, id, "round_1")
            .await.map_err(|e| format!("db error: {e}"))?;
        state_machine::start_round(pool, id, 1).await?;

        rounds::round1::run_round1(
            pool, client, id, active_bots, bot_tokens, &role_assignments,
            &pseudonym_map, round0_context.clone(), timeout,
        ).await?;

        state_machine::complete_round(pool, id, 1).await?;
    }

    // Build Round 1 context
    let r1_responses = queries::get_responses(pool, id, 1)
        .await.map_err(|e| format!("db error: {e}"))?;
    let round1_context: Vec<RoundContext> = r1_responses.iter()
        .filter(|r| !r.abstained)
        .map(|r| {
            let pseudo = pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default();
            RoundContext { pseudonym: pseudo, round: 1, response: r.response_json.clone(), confidence: r.confidence }
        })
        .collect();

    // === ROUND 2 ===
    if resume_round <= 2 {
        queries::update_debate_status(pool, id, "round_2")
            .await.map_err(|e| format!("db error: {e}"))?;
        state_machine::start_round(pool, id, 2).await?;

        rounds::round2::run_round2(
            pool, client, id, active_bots, bot_tokens, &role_assignments,
            round1_context.clone(), models_config, timeout, debate_config.max_retries,
        ).await?;

        state_machine::complete_round(pool, id, 2).await?;
    }

    // Build Round 2 response map for pairing
    let r2_responses = queries::get_responses(pool, id, 2)
        .await.map_err(|e| format!("db error: {e}"))?;
    let round2_responses: HashMap<String, String> = r2_responses.iter()
        .filter(|r| !r.abstained)
        .map(|r| {
            let pseudo = pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default();
            (pseudo, r.response_json.clone())
        })
        .collect();

    // === ROUND 3 ===
    if resume_round <= 3 {
        queries::update_debate_status(pool, id, "round_3")
            .await.map_err(|e| format!("db error: {e}"))?;
        state_machine::start_round(pool, id, 3).await?;

        rounds::round3::run_round3(
            pool, client, id, active_bots, bot_tokens, &role_assignments,
            &pseudonym_map, &reverse_pseudonym_map, &round2_responses,
            models_config, timeout,
        ).await?;

        state_machine::complete_round(pool, id, 3).await?;
    }

    // Build full context for Round 4 (all rounds 0-3)
    let all_prior = queries_phase1::get_all_responses(pool, id)
        .await.map_err(|e| format!("db error: {e}"))?;
    let full_context: Vec<RoundContext> = all_prior.iter()
        .filter(|r| !r.abstained && r.round_number <= 3)
        .map(|r| {
            let pseudo = pseudonym_map.get(&r.bot_id).cloned().unwrap_or_default();
            RoundContext { pseudonym: pseudo, round: r.round_number, response: r.response_json.clone(), confidence: r.confidence }
        })
        .collect();

    // === ROUND 4 ===
    if resume_round <= 4 {
        queries::update_debate_status(pool, id, "round_4")
            .await.map_err(|e| format!("db error: {e}"))?;
        state_machine::start_round(pool, id, 4).await?;

        rounds::round4::run_round4(
            pool, client, id, topic, active_bots, bot_tokens,
            &role_assignments, full_context, timeout,
        ).await?;

        state_machine::complete_round(pool, id, 4).await?;
    }

    // === DIVERGENCE ANALYSIS ===
    queries::update_debate_status(pool, id, "analysing")
        .await.map_err(|e| format!("db error: {e}"))?;

    let r4_responses = queries::get_responses(pool, id, 4)
        .await.map_err(|e| format!("db error: {e}"))?;

    let divergence_futures: Vec<_> = r4_responses.iter()
        .filter(|r| !r.abstained)
        .map(|r4| {
            let bot_id = r4.bot_id.clone();
            let r0_resp = r0_responses.iter()
                .find(|r| r.bot_id == bot_id && !r.abstained)
                .map(|r| r.response_json.clone())
                .unwrap_or_default();
            let r4_resp = r4.response_json.clone();
            let pc_json = r4.position_change_json.clone().unwrap_or_else(|| "{}".into());
            let config = models_config.clone();
            async move {
                let result = analyse_divergence(&config, &r0_resp, &r4_resp, &pc_json).await;
                (bot_id, result)
            }
        })
        .collect();

    let divergence_results = futures::future::join_all(divergence_futures).await;

    // Store divergence analyses
    for (bot_id, result) in &divergence_results {
        match result {
            Ok(div) => {
                let analysis_id = uuid::Uuid::new_v4().to_string();
                let input = serde_json::json!({ "bot_id": bot_id }).to_string();
                let result_json = serde_json::to_string(div).unwrap_or_default();
                let _ = queries_phase1::insert_analysis(
                    pool, &analysis_id, id, Some(bot_id),
                    "divergence", &input, &result_json, &models_config.minimax_model,
                ).await;
            }
            Err(e) => {
                tracing::warn!(bot_id = %bot_id, error = %e, "divergence analysis failed");
            }
        }
    }

    // === SYNTHESIS ===
    queries::update_debate_status(pool, id, "synthesising")
        .await.map_err(|e| format!("db error: {e}"))?;

    let all_responses = queries_phase1::get_all_responses(pool, id)
        .await.map_err(|e| format!("db error: {e}"))?;

    // Build transcript text
    let mut transcript_lines: Vec<String> = Vec::new();
    for resp in &all_responses {
        if resp.abstained { continue; }
        let pseudo = pseudonym_map.get(&resp.bot_id).cloned().unwrap_or_default();
        transcript_lines.push(format!("[{pseudo}, Round {}]: {}", resp.round_number, resp.response_json));
    }
    let transcript_text = transcript_lines.join("\n\n");

    // Pre-compute structural data
    let precomputed = precompute::precompute(&all_responses, &pseudonym_map);
    let precomputed_json = serde_json::to_string(&precomputed).unwrap_or_default();

    // Divergence results JSON
    let div_results: Vec<_> = divergence_results.iter()
        .filter_map(|(bot_id, r)| {
            r.as_ref().ok().map(|d| {
                let pseudo = pseudonym_map.get(bot_id).cloned().unwrap_or_default();
                serde_json::json!({ "pseudonym": pseudo, "analysis": d })
            })
        })
        .collect();
    let divergence_json = serde_json::to_string(&div_results).unwrap_or_default();

    let (synthesis_output, prompt_hash) = synthesiser::run_synthesis(
        models_config, topic, &transcript_text, &precomputed_json,
        &divergence_json, debate_config.synthesis_temperature,
    ).await.map_err(|e| format!("synthesis failed: {e}"))?;

    // Store synthesis
    queries_phase1::insert_synthesis(pool, id, &synthesis_output, &models_config.opus_model, &prompt_hash)
        .await.map_err(|e| format!("db error storing synthesis: {e}"))?;

    // === COMPLETE ===
    queries::update_debate_status(pool, id, "complete")
        .await.map_err(|e| format!("db error: {e}"))?;

    tracing::info!(debate_id = %id, "multi-round debate completed successfully");
    Ok(())
}
```

Note: This file will be over 200 lines. If it exceeds 300 lines, split the divergence + synthesis sections into `src/orchestrator/post_round.rs`. The implementing engineer should check the line count and split if needed.

- [ ] **Step 2: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 3: Commit**

```bash
git add src/orchestrator/mod.rs
git commit -m "feat: multi-round orchestrator — drives Rounds 0-4, divergence, and synthesis"
```

---

## Task 18: Update create_debate Handler — Wire Phase 1 Orchestrator

**Files:**
- Modify: `src/api/debates.rs`

- [ ] **Step 1: Update create_debate to use role assignment and multi-round orchestrator**

In `src/api/debates.rs`, update the `create_debate` function:

1. After inserting debate_bots, call `roles::assign_roles()` and `roles::persist_role_assignments()`
2. Call `state_machine::init_rounds()`
3. Spawn `run_multi_round_debate` instead of `run_debate`
4. Include role in `DebateBotInfo`

Replace the body of `create_debate` from the line `let mut bot_tokens` onward:

```rust
    // Assign roles with rotation
    let bot_ids: Vec<String> = bots.iter().map(|b| b.id.clone()).collect();
    let role_assignments = orchestrator::roles::assign_roles(state.db(), &bot_ids)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    let mut bot_tokens = std::collections::HashMap::new();
    for (i, bot) in bots.iter().enumerate() {
        let pseudonym = anonymiser::assign_pseudonym(i);
        queries::insert_debate_bot(state.db(), debate_id.as_str(), &bot.id, &pseudonym).await?;
        bot_tokens.insert(bot.id.clone(), String::new());
    }

    // Persist role assignments
    orchestrator::roles::persist_role_assignments(state.db(), debate_id.as_str(), &role_assignments)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    // Init round state machine
    orchestrator::state_machine::init_rounds(state.db(), debate_id.as_str())
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    // Spawn multi-round debate as background task
    let pool = state.db().clone();
    let client = state.http_client().clone();
    let topic = req.topic.clone();
    let debate_id_clone = debate_id.clone();
    let bots_clone = bots.clone();
    let models_config = state.settings().models.clone();
    let debate_config = state.settings().debate.clone();
    tokio::spawn(async move {
        if let Err(e) = orchestrator::run_multi_round_debate(
            &pool, &client, &debate_id_clone, &topic, &bots_clone, &bot_tokens,
            &models_config, &debate_config,
        ).await {
            tracing::error!(debate_id = %debate_id_clone, error = %e, "multi-round debate failed");
            let _ = queries::update_debate_status(&pool, debate_id_clone.as_str(), "failed").await;
        }
    });
```

- [ ] **Step 2: Update DebateBotInfo to include role**

In `src/api/dto.rs`, add `role` field to `DebateBotInfo`:

```rust
pub struct DebateBotInfo {
    pub bot_id: String,
    pub bot_name: String,
    pub pseudonym: String,
    pub role: Option<String>,
}
```

Update all places that construct `DebateBotInfo` in `debates.rs` to include the role from `debate_bots`:

```rust
let bot_infos: Vec<DebateBotInfo> = debate_bots.iter().map(|db| {
    let bot = bots.iter().find(|b| b.id == db.bot_id);
    DebateBotInfo {
        bot_id: db.bot_id.clone(),
        bot_name: bot.map(|b| b.name.clone()).unwrap_or_default(),
        pseudonym: db.pseudonym.clone(),
        role: None, // Will be populated after role assignment
    }
}).collect();
```

For the `get_debate` and `list_debates` handlers, use `get_debate_bots_with_roles`:

```rust
let debate_bots = queries_phase1::get_debate_bots_with_roles(state.db(), &id).await?;
// ... construct DebateBotInfo with role: db.role.clone()
```

- [ ] **Step 3: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 4: Commit**

```bash
git add src/api/debates.rs src/api/dto.rs
git commit -m "feat: wire Phase 1 multi-round orchestrator into create_debate handler"
```

---

## Task 19: Transcript and Synthesis API Endpoints

**Files:**
- Create: `src/api/transcript.rs`
- Create: `src/api/synthesis.rs`
- Modify: `src/api/mod.rs`
- Modify: `src/api/dto.rs`

- [ ] **Step 1: Add DTO types for transcript and synthesis**

Add to `src/api/dto.rs`:

```rust
/// Response for GET /debates/{id}/transcript.
#[derive(Debug, Serialize)]
pub struct TranscriptResponse {
    pub debate_id: String,
    pub topic: String,
    pub rounds: Vec<TranscriptRound>,
    pub anonymisation_log: Vec<AnonymisationEntry>,
}

#[derive(Debug, Serialize)]
pub struct TranscriptRound {
    pub round_number: i64,
    pub status: String,
    pub responses: Vec<TranscriptEntry>,
}

#[derive(Debug, Serialize)]
pub struct TranscriptEntry {
    pub pseudonym: String,
    pub response: String,
    pub confidence: Option<i64>,
    pub challenge: Option<serde_json::Value>,
    pub position_change: Option<serde_json::Value>,
    pub valid: bool,
    pub abstained: bool,
}

#[derive(Debug, Serialize)]
pub struct AnonymisationEntry {
    pub pseudonym: String,
    pub role: Option<String>,
}

/// Response for GET /debates/{id}/synthesis.
#[derive(Debug, Serialize)]
pub struct SynthesisResponse {
    pub debate_id: String,
    pub synthesis: serde_json::Value,
    pub model_used: String,
    pub created_at: String,
}
```

- [ ] **Step 2: Create transcript.rs handler**

Create `src/api/transcript.rs`:

```rust
use axum::extract::{Path, State};
use axum::Json;
use crate::api::auth::BearerAuth;
use crate::api::dto::*;
use crate::db::{queries, queries_phase1};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// GET /debates/{id}/transcript — full round-by-round transcript.
pub async fn get_transcript(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Path(id): Path<String>,
) -> AppResult<Json<TranscriptResponse>> {
    let debate = queries::get_debate(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound(format!("debate {id} not found")))?;

    let debate_bots = queries_phase1::get_debate_bots_with_roles(state.db(), &id).await?;
    let rounds = queries_phase1::get_rounds(state.db(), &id).await?;
    let all_responses = queries_phase1::get_all_responses(state.db(), &id).await?;

    let mut transcript_rounds = Vec::new();
    for round in &rounds {
        let round_responses: Vec<TranscriptEntry> = all_responses.iter()
            .filter(|r| r.round_number == round.round_number)
            .map(|r| {
                let pseudonym = debate_bots.iter()
                    .find(|db| db.bot_id == r.bot_id)
                    .map(|db| db.pseudonym.clone())
                    .unwrap_or_else(|| "Unknown".into());
                let challenge = r.challenge_json.as_ref()
                    .and_then(|c| serde_json::from_str(c).ok());
                let position_change = r.position_change_json.as_ref()
                    .and_then(|p| serde_json::from_str(p).ok());
                TranscriptEntry {
                    pseudonym,
                    response: r.response_json.clone(),
                    confidence: r.confidence,
                    challenge,
                    position_change,
                    valid: r.valid,
                    abstained: r.abstained,
                }
            })
            .collect();

        transcript_rounds.push(TranscriptRound {
            round_number: round.round_number,
            status: round.status.clone(),
            responses: round_responses,
        });
    }

    let anonymisation_log: Vec<AnonymisationEntry> = debate_bots.iter()
        .map(|db| AnonymisationEntry {
            pseudonym: db.pseudonym.clone(),
            role: db.role.clone(),
        })
        .collect();

    Ok(Json(TranscriptResponse {
        debate_id: id,
        topic: debate.topic,
        rounds: transcript_rounds,
        anonymisation_log,
    }))
}
```

- [ ] **Step 3: Create synthesis.rs handler**

Create `src/api/synthesis.rs`:

```rust
use axum::extract::{Path, State};
use axum::Json;
use crate::api::auth::BearerAuth;
use crate::api::dto::*;
use crate::db::{queries, queries_phase1};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

/// GET /debates/{id}/synthesis — final synthesis output (404 if not yet complete).
pub async fn get_synthesis(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Path(id): Path<String>,
) -> AppResult<Json<SynthesisResponse>> {
    // Verify debate exists
    let _debate = queries::get_debate(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound(format!("debate {id} not found")))?;

    let synthesis = queries_phase1::get_synthesis(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound(format!("synthesis not yet available for debate {id}")))?;

    let output: serde_json::Value = serde_json::from_str(&synthesis.output_json)
        .unwrap_or_else(|_| serde_json::Value::String(synthesis.output_json.clone()));

    Ok(Json(SynthesisResponse {
        debate_id: id,
        synthesis: output,
        model_used: synthesis.model_used,
        created_at: synthesis.created_at,
    }))
}
```

- [ ] **Step 4: Register routes in api/mod.rs**

Update `src/api/mod.rs`:

```rust
pub mod auth;
pub mod bots;
pub mod debates;
pub mod dto;
pub mod health;
pub mod transcript;
pub mod synthesis;

use axum::{Router, routing::get};
use crate::state::AppState;

/// Build the API router with all routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/bots", get(bots::list_bots).post(bots::create_bot))
        .route("/debates", get(debates::list_debates).post(debates::create_debate))
        .route("/debates/{id}", get(debates::get_debate))
        .route("/debates/{id}/transcript", get(transcript::get_transcript))
        .route("/debates/{id}/synthesis", get(synthesis::get_synthesis))
        .with_state(state)
}
```

- [ ] **Step 5: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 6: Commit**

```bash
git add src/api/transcript.rs src/api/synthesis.rs src/api/mod.rs src/api/dto.rs
git commit -m "feat: transcript and synthesis API endpoints"
```

---

## Task 20: Integration Tests — Phase 1 API

**Files:**
- Modify: `tests/api_debates_test.rs`

- [ ] **Step 1: Add transcript and synthesis endpoint tests**

Add to `tests/api_debates_test.rs`:

```rust
#[tokio::test]
async fn test_get_transcript_not_found() {
    let (app, _pool) = common::test_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/debates/nonexistent/transcript")
                .header("Authorization", "Bearer test")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_get_synthesis_not_found() {
    let (app, _pool) = common::test_app().await;
    let response = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/debates/nonexistent/synthesis")
                .header("Authorization", "Bearer test")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}
```

- [ ] **Step 2: Sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

Expected: 8 tests pass (6 existing + 2 new).

- [ ] **Step 3: Commit**

```bash
git add tests/api_debates_test.rs
git commit -m "test: transcript and synthesis endpoint 404 tests"
```

---

## Task 21: Update Reference Bot Endpoints

**Files:**
- Modify: `reference/debate-endpoint-node.js`
- Modify: `reference/debate-endpoint-python.py`

- [ ] **Step 1: Update Node.js reference endpoint to handle Phase 1 fields**

Replace `reference/debate-endpoint-node.js`:

```javascript
const http = require('http');

const server = http.createServer((req, res) => {
  if (req.method !== 'POST' || req.url !== '/debate') {
    res.writeHead(404, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ error: 'not found' }));
    return;
  }

  let body = '';
  req.on('data', chunk => { body += chunk; });
  req.on('end', () => {
    const data = JSON.parse(body);
    const round = data.round;
    const role = data.role || 'unknown';

    let response;

    if (round === 0) {
      // Blind formation
      response = {
        response: `[${role}] My initial position on "${data.prompt.slice(0, 100)}..." is that this requires careful analysis from multiple angles.`,
      };
    } else if (round === 1) {
      // Anonymous distribution
      response = {
        response: `The strongest opposing argument comes from the empirical evidence cited. I would change my position if presented with verified data contradicting my core claim.`,
        confidence: 65,
      };
    } else if (round === 2) {
      // Structured rebuttal
      response = {
        response: `I challenge the assumption that the evidence presented is conclusive. The methodology has significant gaps.`,
        confidence: 70,
        challenge: {
          claim_targeted: 'The claim that the evidence is conclusive',
          counter_evidence: 'The sample size is insufficient and the control group was not properly isolated.',
          type: 'factual',
        },
      };
    } else if (round === 3) {
      // Cross-examination
      response = {
        response: `My question: What assumption does your position rely on that, if false, would invalidate your entire argument? My answer: The core assumption is falsifiable through longitudinal study.`,
        confidence: 68,
      };
    } else if (round === 4) {
      // Final position
      response = {
        response: `My final position remains that careful empirical analysis is required. The debate has refined but not fundamentally altered my view.`,
        confidence: 72,
        position_change: {
          changed: false,
          from_summary: 'Careful analysis required',
          to_summary: 'Careful analysis required, with refined methodology criteria',
          reason: 'The opposing arguments raised valid methodological concerns but did not undermine the core thesis.',
        },
      };
    } else if (round === 'scoring') {
      // Phase 0 backward compat
      const scores = (data.context || []).map(entry => ({
        pseudonym: entry.pseudonym,
        reasoning_quality: 7,
        factual_grounding: 6,
        overall: 7,
        reasoning: 'Solid argument with room for improvement.',
      }));
      response = { scores };
    } else {
      response = { response: 'Unknown round', confidence: 50 };
    }

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify(response));
  });
});

server.listen(3200, () => console.log('Reference bot listening on :3200'));
```

- [ ] **Step 2: Update Python reference endpoint**

Replace `reference/debate-endpoint-python.py`:

```python
from http.server import HTTPServer, BaseHTTPRequestHandler
import json

class DebateHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != '/debate':
            self.send_response(404)
            self.end_headers()
            self.wfile.write(json.dumps({"error": "not found"}).encode())
            return

        length = int(self.headers.get('Content-Length', 0))
        body = json.loads(self.rfile.read(length))
        round_num = body.get('round')
        role = body.get('role', 'unknown')

        if round_num == 0:
            response = {
                "response": f"[{role}] Initial position: This topic requires rigorous empirical analysis.",
            }
        elif round_num == 1:
            response = {
                "response": "The strongest opposing argument is the appeal to historical precedent. I would reconsider if shown systematic evidence of a different pattern.",
                "confidence": 60,
            }
        elif round_num == 2:
            response = {
                "response": "I challenge the reliance on anecdotal evidence rather than systematic study.",
                "confidence": 65,
                "challenge": {
                    "claim_targeted": "The assertion based on anecdotal evidence",
                    "counter_evidence": "Systematic reviews show no consistent pattern matching the anecdotal claims.",
                    "type": "factual",
                },
            }
        elif round_num == 3:
            response = {
                "response": "Question: What would constitute sufficient evidence to falsify your position? Answer: A controlled study with adequate sample size.",
                "confidence": 63,
            }
        elif round_num == 4:
            response = {
                "response": "Final position: Empirical rigour remains the correct lens. The debate has sharpened the methodological requirements.",
                "confidence": 70,
                "position_change": {
                    "changed": True,
                    "from_summary": "General call for empirical analysis",
                    "to_summary": "Specific methodological requirements identified",
                    "reason": "Agent B's challenge about sample size validity was compelling.",
                },
            }
        elif round_num == "scoring":
            scores = [
                {
                    "pseudonym": entry["pseudonym"],
                    "reasoning_quality": 7,
                    "factual_grounding": 6,
                    "overall": 7,
                    "reasoning": "Adequate reasoning.",
                }
                for entry in body.get("context", [])
            ]
            response = {"scores": scores}
        else:
            response = {"response": "Unknown round", "confidence": 50}

        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(response).encode())

HTTPServer(('', 3201), DebateHandler).serve_forever()
```

- [ ] **Step 3: Commit**

```bash
git add reference/
git commit -m "ref: update reference bot endpoints for Phase 1 multi-round protocol"
```

---

## Task 22: E2E Smoke Test Script

**Files:**
- Modify: `reference/run-smoke-test.sh`

- [ ] **Step 1: Update smoke test for Phase 1**

Replace `reference/run-smoke-test.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

BASE_URL="http://localhost:3100"
TOKEN="test-token"

echo "=== Bot Council Phase 1 Smoke Test ==="

# Register 5 bots (minimum for full role assignment)
for i in 1 2 3 4 5; do
  PORT=$((3199 + i))
  echo "Registering bot-${i}..."
  curl -s -X POST "${BASE_URL}/bots" \
    -H "Authorization: Bearer ${TOKEN}" \
    -H "Content-Type: application/json" \
    -d "{\"name\":\"smoke-bot-${i}\",\"endpoint_url\":\"http://localhost:${PORT}/debate\",\"token\":\"bot-token-${i}\"}" | jq .
done

echo ""
echo "Listing bots..."
BOTS=$(curl -s "${BASE_URL}/bots" -H "Authorization: Bearer ${TOKEN}")
echo "${BOTS}" | jq '.[] | {id, name}'
BOT_IDS=$(echo "${BOTS}" | jq -r '.[].id' | head -5 | tr '\n' ',' | sed 's/,$//')

echo ""
echo "Creating multi-round debate..."
DEBATE=$(curl -s -X POST "${BASE_URL}/debates" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{\"topic\":\"Should AI systems be required to explain their reasoning?\"}")
echo "${DEBATE}" | jq .
DEBATE_ID=$(echo "${DEBATE}" | jq -r '.id')

echo ""
echo "Waiting for debate to complete (5-round protocol)..."
for i in $(seq 1 60); do
  STATUS=$(curl -s "${BASE_URL}/debates/${DEBATE_ID}" -H "Authorization: Bearer ${TOKEN}" | jq -r '.status')
  echo "  Status: ${STATUS}"
  if [ "${STATUS}" = "complete" ] || [ "${STATUS}" = "failed" ]; then
    break
  fi
  sleep 5
done

echo ""
echo "Fetching transcript..."
curl -s "${BASE_URL}/debates/${DEBATE_ID}/transcript" -H "Authorization: Bearer ${TOKEN}" | jq '.rounds | length'

echo ""
echo "Fetching synthesis..."
curl -s "${BASE_URL}/debates/${DEBATE_ID}/synthesis" -H "Authorization: Bearer ${TOKEN}" | jq .

echo ""
echo "=== Smoke test complete ==="
```

- [ ] **Step 2: Commit**

```bash
git add reference/run-smoke-test.sh
git commit -m "test: update smoke test script for Phase 1 multi-round protocol"
```

---

## Task 23: Final Compilation Check and Deploy Test

- [ ] **Step 1: Full sync and test**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests config migrations reference Cargo.toml Cargo.lock james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

Expected: all tests pass (8 minimum — 6 Phase 0 + 2 new endpoint tests).

- [ ] **Step 2: Fix any compilation errors**

Address any errors surfaced by the full build. Common issues:
- Missing imports (check `use` statements in each file)
- Type mismatches between tasks (e.g., `DebateBotRow` vs `DebateBotWithRoleRow`)
- Module re-export issues

- [ ] **Step 3: Update CLAUDE.md with Phase 1 endpoints**

Add to the API Endpoints table in `CLAUDE.md`:

```markdown
| GET | /debates/{id}/transcript | Full transcript with anonymisation log |
| GET | /debates/{id}/synthesis | Synthesis output (404 if incomplete) |
```

Update "Current Phase" to:

```markdown
## Current Phase: 1 (Multi-Round Protocol)

Phase 1 supports: constitutional roles with rotation, 5-round adversarial protocol (blind formation,
anonymous distribution, structured rebuttal with MiniMax validation, cross-examination with
MiniMax pairing, final position with position change tracking), divergence analysis, Opus synthesis.
```

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for Phase 1 — endpoints, phase status, architecture"
```

---

## Dependency Graph

```
Task 1 (migration) ─────────┐
Task 2 (config) ─────────────┤
Task 3 (types) ──────────────┤
Task 5 (rand dep) ───────────┤
                              ├── Task 4 (db models/queries)
                              │
Task 4 ──────────────────────├── Task 6 (roles)
                              ├── Task 7 (prompts)
                              ├── Task 8 (bot client types)
                              ├── Task 9 (error types)
                              │
Task 8, Task 10 ─────────────├── Task 10 (analyser/mod.rs)
Task 10 ─────────────────────├── Task 11 (challenge validation)
Task 10 ─────────────────────├── Task 12 (pairing)
Task 10 ─────────────────────├── Task 13 (divergence)
                              │
Task 14 (state machine) ─────┤
                              │
Tasks 6-14 ──────────────────├── Task 15 (rounds 0-4)
Tasks 13, 15, 16 ────────────├── Task 16 (synthesis)
                              │
Tasks 15-16 ─────────────────├── Task 17 (orchestrator rewrite)
Task 17 ─────────────────────├── Task 18 (handler wiring)
Task 4 ──────────────────────├── Task 19 (transcript/synthesis API)
                              │
Task 19 ─────────────────────├── Task 20 (integration tests)
                              ├── Task 21 (reference bots)
                              ├── Task 22 (smoke test)
                              │
All tasks ───────────────────── Task 23 (final verification)
```

Tasks 1-3, 5 can run in parallel.
Tasks 6-14 can mostly run in parallel (after Task 4).
Tasks 15-16 depend on the analyser and earlier modules.
Task 17 depends on everything.
Tasks 18-22 depend on 17 and can partially parallelize.
Task 23 is the final gate.
