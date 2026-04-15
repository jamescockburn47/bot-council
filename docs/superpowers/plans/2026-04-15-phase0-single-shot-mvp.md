# Phase 0: Single-Shot MVP — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a working Rust/Axum harness that can register bots, create single-shot debates, dispatch topics to bots via HTTP, anonymise responses, collect peer scores, and return ranked results.

**Architecture:** Standalone Rust binary using Axum 0.8 for HTTP, sqlx 0.8 for SQLite persistence, reqwest 0.12 for outbound bot calls. AppState holds DB pool and HTTP client. Repository pattern — handlers call db::queries, never raw SQL. Newtype IDs, thiserror for domain errors.

**Tech Stack:** Rust 2024 edition, Axum 0.8, Tokio, sqlx 0.8 (SQLite), reqwest 0.12 with reqwest-middleware/retry, serde/serde_json, thiserror, anyhow, tracing, config 0.14, uuid, chrono, sha2 (token hashing).

**Coding rules (from spec — binding on all tasks):**
- Max 300 lines per file. Split before adding.
- One file, one job. Single responsibility.
- No `unwrap()` in production paths. `?` operator or explicit handling.
- No `.ok()` without `// intentional: [reason]` comment.
- Newtype wrappers for IDs: `DebateId(String)`, `BotId(String)`.
- Enums for fixed values with serde derive.
- All constants in `config.rs` or constants modules. Zero `std::env` outside config.
- Repository pattern: handlers never touch SQLite directly.
- `thiserror` for domain errors, `anyhow` at binary boundary only.
- Log errors with context via `tracing` structured fields.
- Concurrent dispatch where independent (`join_all`).
- `///` doc comments on all public functions and types.
- Integration tests via `tower::ServiceExt::oneshot` with in-memory SQLite.
- Atomic commits. One logical change per commit.

---

## File Structure

```
bot-council/
  Cargo.toml
  config/
    default.toml
  migrations/
    20260415000001_init.sql
  src/
    main.rs              -- tokio::main, build router, run server (~50 lines)
    lib.rs               -- re-exports build_router(), AppState construction (~30 lines)
    config.rs            -- Settings struct, TOML + env var loading (~60 lines)
    error.rs             -- AppError enum, IntoResponse impl (~70 lines)
    state.rs             -- AppState definition (Arc<Inner> pattern) (~40 lines)
    types.rs             -- Newtype IDs (DebateId, BotId), common enums (~50 lines)
    api/
      mod.rs             -- Router::new() assembly (~40 lines)
      bots.rs            -- POST /bots, GET /bots handlers (~80 lines)
      debates.rs         -- POST /debates, GET /debates, GET /debates/{id} handlers (~120 lines)
      health.rs          -- GET /health handler (~20 lines)
      auth.rs            -- BearerAuth extractor (~40 lines)
      dto.rs             -- Request/response DTOs (~100 lines)
    orchestrator/
      mod.rs             -- run_debate() — dispatches, anonymises, scores, aggregates (~150 lines)
      anonymiser.rs      -- strip identity, assign pseudonyms, log mapping (~60 lines)
    bot_client/
      mod.rs             -- BotClient: send_position_request, send_scoring_request (~100 lines)
    db/
      mod.rs             -- pool init, run migrations (~30 lines)
      models.rs          -- Row structs matching DB tables (~60 lines)
      queries.rs         -- insert/select/update functions (~150 lines)
  tests/
    common/
      mod.rs             -- test helpers: build test app, test state, seed data (~80 lines)
    api_bots_test.rs     -- integration tests for /bots endpoints (~80 lines)
    api_debates_test.rs  -- integration tests for /debates endpoints (~120 lines)
    api_health_test.rs   -- integration test for /health (~20 lines)
    orchestrator_test.rs -- unit tests for debate orchestration (~150 lines)
  reference/
    debate-endpoint-node.js   -- Reference /debate endpoint (Node.js) (~60 lines)
    debate-endpoint-python.py -- Reference /debate endpoint (Python) (~50 lines)
```

---

## Task 1: Project Scaffold and Dependencies

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `.gitignore`

- [ ] **Step 1: Initialise Cargo project**

```bash
cd "C:/Users/James/Desktop/LQ projects/Bot council"
cargo init --name bot-council
```

- [ ] **Step 2: Replace Cargo.toml with full dependency set**

```toml
[package]
name = "bot-council"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8"
axum-extra = { version = "0.10", features = ["typed-header"] }
tokio = { version = "1", features = ["full"] }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["cors", "trace"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "migrate"] }
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
reqwest = { version = "0.12", features = ["json"] }
reqwest-middleware = "0.4"
reqwest-retry = "0.7"
config = "0.14"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
sha2 = "0.10"
hex = "0.4"

[dev-dependencies]
tokio = { version = "1", features = ["full", "test-util"] }
tower = { version = "0.5", features = ["util"] }
wiremock = "0.6"
```

- [ ] **Step 3: Write minimal main.rs**

```rust
// src/main.rs
use bot_council::build_app;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let app = build_app().await?;
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3100").await?;
    tracing::info!("Bot Council listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 4: Write minimal lib.rs**

```rust
// src/lib.rs
pub mod api;
pub mod config;
pub mod db;
pub mod error;
pub mod state;
pub mod types;
pub mod bot_client;
pub mod orchestrator;

use axum::Router;

/// Build the full application router with state.
pub async fn build_app() -> anyhow::Result<Router> {
    let settings = config::Settings::load()?;
    let pool = db::init_pool(&settings.database.url).await?;
    let http_client = bot_client::build_http_client(&settings.http_client);
    let state = state::AppState::new(pool, http_client, settings.clone());
    Ok(api::router(state))
}
```

- [ ] **Step 5: Add .gitignore**

```gitignore
/target
data/
*.db
*.db-journal
*.db-wal
node_modules/
.env
```

- [ ] **Step 6: Verify it compiles (expect module errors — that's fine)**

```bash
cargo check 2>&1 | head -20
```

Expected: compilation errors about missing modules. This confirms Cargo.toml is valid and dependencies resolve.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs src/lib.rs .gitignore
git commit -m "feat: project scaffold with dependencies"
```

---

## Task 2: Config, Error, Types, State

**Files:**
- Create: `src/config.rs`
- Create: `src/error.rs`
- Create: `src/types.rs`
- Create: `src/state.rs`
- Create: `config/default.toml`

- [ ] **Step 1: Write config.rs**

```rust
// src/config.rs
use serde::Deserialize;

/// Top-level application settings. Loaded from config/default.toml + env vars.
#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub http_client: HttpClientConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub admin_token: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HttpClientConfig {
    pub connect_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub max_retries: u32,
    pub retry_delay_secs: u64,
}

impl Settings {
    /// Load settings from config/default.toml, overridden by APP__* env vars.
    pub fn load() -> anyhow::Result<Self> {
        let config = config::Config::builder()
            .add_source(config::File::with_name("config/default"))
            .add_source(
                config::Environment::with_prefix("APP")
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?;
        Ok(config.try_deserialize()?)
    }
}
```

- [ ] **Step 2: Write config/default.toml**

```toml
[server]
host = "0.0.0.0"
port = 3100

[database]
url = "sqlite:data/council.db?mode=rwc"

[auth]
admin_token = ""

[http_client]
connect_timeout_secs = 5
request_timeout_secs = 300
max_retries = 1
retry_delay_secs = 10
```

- [ ] **Step 3: Write error.rs**

```rust
// src/error.rs
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Domain error type. Every variant maps to an HTTP status + JSON body.
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("database: {0}")]
    Database(#[from] sqlx::Error),

    #[error("bot unreachable: {0}")]
    BotUnreachable(String),

    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".into()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            AppError::BotUnreachable(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::Database(e) => {
                tracing::error!(error = %e, "database error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
            AppError::Internal(e) => {
                tracing::error!(error = %e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

/// Alias for handler return types.
pub type AppResult<T> = Result<T, AppError>;
```

- [ ] **Step 4: Write types.rs**

```rust
// src/types.rs
use serde::{Deserialize, Serialize};
use std::fmt;

/// Newtype wrapper for debate IDs. Prevents mixing with other string IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DebateId(pub String);

impl DebateId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DebateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Newtype wrapper for bot IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BotId(pub String);

impl BotId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Debate status enum. Used in DB and API responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DebateStatus {
    Created,
    Dispatching,
    Scoring,
    Complete,
    Cancelled,
    Failed,
}

impl DebateStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Created => "created",
            Self::Dispatching => "dispatching",
            Self::Scoring => "scoring",
            Self::Complete => "complete",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "created" => Some(Self::Created),
            "dispatching" => Some(Self::Dispatching),
            "scoring" => Some(Self::Scoring),
            "complete" => Some(Self::Complete),
            "cancelled" => Some(Self::Cancelled),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}
```

- [ ] **Step 5: Write state.rs**

```rust
// src/state.rs
use std::sync::Arc;
use sqlx::SqlitePool;
use crate::config::Settings;

/// Application state shared across all handlers. Cheap to clone (Arc wrapper).
#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    pub db: SqlitePool,
    pub http_client: reqwest_middleware::ClientWithMiddleware,
    pub settings: Settings,
}

impl AppState {
    pub fn new(
        db: SqlitePool,
        http_client: reqwest_middleware::ClientWithMiddleware,
        settings: Settings,
    ) -> Self {
        Self {
            inner: Arc::new(AppStateInner { db, http_client, settings }),
        }
    }

    pub fn db(&self) -> &SqlitePool {
        &self.inner.db
    }

    pub fn http_client(&self) -> &reqwest_middleware::ClientWithMiddleware {
        &self.inner.http_client
    }

    pub fn settings(&self) -> &Settings {
        &self.inner.settings
    }
}
```

- [ ] **Step 6: Verify compilation**

```bash
cargo check
```

Expected: errors about missing `api`, `db`, `bot_client`, `orchestrator` modules. Config/error/types/state should compile cleanly.

- [ ] **Step 7: Commit**

```bash
git add src/config.rs src/error.rs src/types.rs src/state.rs config/default.toml
git commit -m "feat: config, error types, newtype IDs, app state"
```

---

## Task 3: Database Layer

**Files:**
- Create: `migrations/20260415000001_init.sql`
- Create: `src/db/mod.rs`
- Create: `src/db/models.rs`
- Create: `src/db/queries.rs`

- [ ] **Step 1: Write migration SQL (Phase 0 tables only)**

```sql
-- migrations/20260415000001_init.sql

-- Bot registry
CREATE TABLE IF NOT EXISTS bots (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    endpoint_url TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    model_family TEXT,
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Debate sessions
CREATE TABLE IF NOT EXISTS debates (
    id TEXT PRIMARY KEY,
    topic TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'created',
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

-- Bot participation in a debate
CREATE TABLE IF NOT EXISTS debate_bots (
    debate_id TEXT NOT NULL REFERENCES debates(id),
    bot_id TEXT NOT NULL REFERENCES bots(id),
    pseudonym TEXT NOT NULL,
    PRIMARY KEY (debate_id, bot_id)
);

-- Individual bot responses
CREATE TABLE IF NOT EXISTS responses (
    id TEXT PRIMARY KEY,
    debate_id TEXT NOT NULL REFERENCES debates(id),
    round_number INTEGER NOT NULL,
    bot_id TEXT NOT NULL REFERENCES bots(id),
    response_json TEXT NOT NULL,
    abstained INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Peer scores (Phase 0: bots score each other)
CREATE TABLE IF NOT EXISTS peer_scores (
    id TEXT PRIMARY KEY,
    debate_id TEXT NOT NULL REFERENCES debates(id),
    scorer_bot_id TEXT NOT NULL REFERENCES bots(id),
    target_pseudonym TEXT NOT NULL,
    reasoning_quality INTEGER NOT NULL,
    factual_grounding INTEGER NOT NULL,
    overall INTEGER NOT NULL,
    reasoning TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

- [ ] **Step 2: Write db/mod.rs**

```rust
// src/db/mod.rs
pub mod models;
pub mod queries;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;

/// Initialise the SQLite connection pool and run migrations.
pub async fn init_pool(url: &str) -> anyhow::Result<SqlitePool> {
    // Ensure data directory exists
    if let Some(path) = url.strip_prefix("sqlite:") {
        let path = path.split('?').next().unwrap_or(path);
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(url)
        .await?;

    // SQLite pragmas for production
    sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
    sqlx::query("PRAGMA synchronous=NORMAL").execute(&pool).await?;
    sqlx::query("PRAGMA busy_timeout=5000").execute(&pool).await?;
    sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!("database initialised");
    Ok(pool)
}
```

- [ ] **Step 3: Write db/models.rs**

```rust
// src/db/models.rs
use serde::Serialize;

/// Row struct for the bots table.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BotRow {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub token_hash: String,
    pub model_family: Option<String>,
    pub active: bool,
    pub created_at: String,
}

/// Row struct for the debates table.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DebateRow {
    pub id: String,
    pub topic: String,
    pub status: String,
    pub config_json: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Row struct for debate_bots join table.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DebateBotRow {
    pub debate_id: String,
    pub bot_id: String,
    pub pseudonym: String,
}

/// Row struct for responses table.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ResponseRow {
    pub id: String,
    pub debate_id: String,
    pub round_number: i64,
    pub bot_id: String,
    pub response_json: String,
    pub abstained: bool,
    pub created_at: String,
}

/// Row struct for peer_scores table.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PeerScoreRow {
    pub id: String,
    pub debate_id: String,
    pub scorer_bot_id: String,
    pub target_pseudonym: String,
    pub reasoning_quality: i64,
    pub factual_grounding: i64,
    pub overall: i64,
    pub reasoning: String,
    pub created_at: String,
}
```

- [ ] **Step 4: Write db/queries.rs**

```rust
// src/db/queries.rs
use sqlx::SqlitePool;
use crate::db::models::*;

// ---- Bots ----

/// Insert a new bot. Returns the inserted row.
pub async fn insert_bot(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    endpoint_url: &str,
    token_hash: &str,
    model_family: Option<&str>,
) -> Result<BotRow, sqlx::Error> {
    sqlx::query_as::<_, BotRow>(
        "INSERT INTO bots (id, name, endpoint_url, token_hash, model_family) VALUES (?, ?, ?, ?, ?) RETURNING *"
    )
    .bind(id).bind(name).bind(endpoint_url).bind(token_hash).bind(model_family)
    .fetch_one(pool)
    .await
}

/// List all active bots.
pub async fn list_active_bots(pool: &SqlitePool) -> Result<Vec<BotRow>, sqlx::Error> {
    sqlx::query_as::<_, BotRow>("SELECT * FROM bots WHERE active = 1 ORDER BY created_at")
        .fetch_all(pool)
        .await
}

/// Get a bot by ID.
pub async fn get_bot(pool: &SqlitePool, id: &str) -> Result<Option<BotRow>, sqlx::Error> {
    sqlx::query_as::<_, BotRow>("SELECT * FROM bots WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Get multiple bots by IDs.
pub async fn get_bots_by_ids(pool: &SqlitePool, ids: &[String]) -> Result<Vec<BotRow>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!("SELECT * FROM bots WHERE id IN ({}) AND active = 1", placeholders);
    let mut q = sqlx::query_as::<_, BotRow>(&query);
    for id in ids {
        q = q.bind(id);
    }
    q.fetch_all(pool).await
}

// ---- Debates ----

/// Insert a new debate.
pub async fn insert_debate(
    pool: &SqlitePool,
    id: &str,
    topic: &str,
) -> Result<DebateRow, sqlx::Error> {
    sqlx::query_as::<_, DebateRow>(
        "INSERT INTO debates (id, topic) VALUES (?, ?) RETURNING *"
    )
    .bind(id).bind(topic)
    .fetch_one(pool)
    .await
}

/// Get a debate by ID.
pub async fn get_debate(pool: &SqlitePool, id: &str) -> Result<Option<DebateRow>, sqlx::Error> {
    sqlx::query_as::<_, DebateRow>("SELECT * FROM debates WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// List debates, optionally filtered by status.
pub async fn list_debates(
    pool: &SqlitePool,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<DebateRow>, sqlx::Error> {
    match status {
        Some(s) => {
            sqlx::query_as::<_, DebateRow>(
                "SELECT * FROM debates WHERE status = ? ORDER BY created_at DESC LIMIT ?"
            )
            .bind(s).bind(limit)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query_as::<_, DebateRow>(
                "SELECT * FROM debates ORDER BY created_at DESC LIMIT ?"
            )
            .bind(limit)
            .fetch_all(pool)
            .await
        }
    }
}

/// Update debate status.
pub async fn update_debate_status(
    pool: &SqlitePool,
    id: &str,
    status: &str,
) -> Result<(), sqlx::Error> {
    let completed_at = if status == "complete" || status == "failed" || status == "cancelled" {
        Some(chrono::Utc::now().to_rfc3339())
    } else {
        None
    };
    sqlx::query("UPDATE debates SET status = ?, completed_at = COALESCE(?, completed_at) WHERE id = ?")
        .bind(status).bind(completed_at).bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ---- Debate Bots ----

/// Insert bot participation in a debate.
pub async fn insert_debate_bot(
    pool: &SqlitePool,
    debate_id: &str,
    bot_id: &str,
    pseudonym: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO debate_bots (debate_id, bot_id, pseudonym) VALUES (?, ?, ?)")
        .bind(debate_id).bind(bot_id).bind(pseudonym)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get all bots in a debate with their pseudonyms.
pub async fn get_debate_bots(
    pool: &SqlitePool,
    debate_id: &str,
) -> Result<Vec<DebateBotRow>, sqlx::Error> {
    sqlx::query_as::<_, DebateBotRow>(
        "SELECT * FROM debate_bots WHERE debate_id = ?"
    )
    .bind(debate_id)
    .fetch_all(pool)
    .await
}

// ---- Responses ----

/// Insert a bot response.
pub async fn insert_response(
    pool: &SqlitePool,
    id: &str,
    debate_id: &str,
    round_number: i64,
    bot_id: &str,
    response_json: &str,
    abstained: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO responses (id, debate_id, round_number, bot_id, response_json, abstained) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(id).bind(debate_id).bind(round_number).bind(bot_id).bind(response_json).bind(abstained)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get all responses for a debate and round.
pub async fn get_responses(
    pool: &SqlitePool,
    debate_id: &str,
    round_number: i64,
) -> Result<Vec<ResponseRow>, sqlx::Error> {
    sqlx::query_as::<_, ResponseRow>(
        "SELECT * FROM responses WHERE debate_id = ? AND round_number = ?"
    )
    .bind(debate_id).bind(round_number)
    .fetch_all(pool)
    .await
}

// ---- Peer Scores ----

/// Insert a peer score.
pub async fn insert_peer_score(
    pool: &SqlitePool,
    id: &str,
    debate_id: &str,
    scorer_bot_id: &str,
    target_pseudonym: &str,
    reasoning_quality: i64,
    factual_grounding: i64,
    overall: i64,
    reasoning: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO peer_scores (id, debate_id, scorer_bot_id, target_pseudonym, reasoning_quality, factual_grounding, overall, reasoning) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(id).bind(debate_id).bind(scorer_bot_id).bind(target_pseudonym)
    .bind(reasoning_quality).bind(factual_grounding).bind(overall).bind(reasoning)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get all peer scores for a debate.
pub async fn get_peer_scores(
    pool: &SqlitePool,
    debate_id: &str,
) -> Result<Vec<PeerScoreRow>, sqlx::Error> {
    sqlx::query_as::<_, PeerScoreRow>(
        "SELECT * FROM peer_scores WHERE debate_id = ?"
    )
    .bind(debate_id)
    .fetch_all(pool)
    .await
}
```

- [ ] **Step 5: Verify compilation**

```bash
cargo check
```

Expected: still errors for missing api/bot_client/orchestrator modules, but db module should compile.

- [ ] **Step 6: Commit**

```bash
git add migrations/ src/db/
git commit -m "feat: database layer — migrations, models, queries"
```

---

## Task 4: Auth Extractor and API Scaffold

**Files:**
- Create: `src/api/mod.rs`
- Create: `src/api/auth.rs`
- Create: `src/api/health.rs`
- Create: `src/api/dto.rs`

- [ ] **Step 1: Write api/auth.rs**

```rust
// src/api/auth.rs
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use crate::error::AppError;
use crate::state::AppState;

/// Extractor that validates Bearer token against config.
pub struct BearerAuth;

impl FromRequestParts<AppState> for BearerAuth {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let expected = &state.settings().auth.admin_token;

        // If no token configured, auth is disabled (dev mode)
        if expected.is_empty() {
            return Ok(Self);
        }

        let header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized)?;

        if token == expected {
            Ok(Self)
        } else {
            Err(AppError::Unauthorized)
        }
    }
}
```

- [ ] **Step 2: Write api/health.rs**

```rust
// src/api/health.rs
use axum::Json;
use axum::extract::State;
use serde_json::{json, Value};
use crate::state::AppState;
use crate::error::AppResult;

/// GET /health — service health + DB connectivity.
pub async fn health(State(state): State<AppState>) -> AppResult<Json<Value>> {
    // Verify DB is reachable
    sqlx::query("SELECT 1").execute(state.db()).await?;
    Ok(Json(json!({ "status": "ok" })))
}
```

- [ ] **Step 3: Write api/dto.rs**

```rust
// src/api/dto.rs
use serde::{Deserialize, Serialize};

// ---- Bot DTOs ----

#[derive(Debug, Deserialize)]
pub struct CreateBotRequest {
    pub name: String,
    pub endpoint_url: String,
    pub token: String,
    pub model_family: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BotResponse {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub model_family: Option<String>,
    pub active: bool,
    pub created_at: String,
}

// ---- Debate DTOs ----

#[derive(Debug, Deserialize)]
pub struct CreateDebateRequest {
    pub topic: String,
    pub bot_ids: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct DebateResponse {
    pub id: String,
    pub topic: String,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub bots: Vec<DebateBotInfo>,
    pub results: Option<DebateResults>,
}

#[derive(Debug, Serialize)]
pub struct DebateBotInfo {
    pub bot_id: String,
    pub bot_name: String,
    pub pseudonym: String,
}

#[derive(Debug, Serialize)]
pub struct DebateResults {
    pub responses: Vec<AnonymisedResponse>,
    pub rankings: Vec<RankedArgument>,
}

#[derive(Debug, Serialize)]
pub struct AnonymisedResponse {
    pub pseudonym: String,
    pub response: String,
    pub abstained: bool,
}

#[derive(Debug, Serialize)]
pub struct RankedArgument {
    pub pseudonym: String,
    pub avg_reasoning_quality: f64,
    pub avg_factual_grounding: f64,
    pub avg_overall: f64,
    pub total_scores: usize,
}

// ---- Debate list ----

#[derive(Debug, Deserialize)]
pub struct ListDebatesQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
}
```

- [ ] **Step 4: Write api/mod.rs (scaffold — handlers added in next tasks)**

```rust
// src/api/mod.rs
pub mod auth;
pub mod bots;
pub mod debates;
pub mod dto;
pub mod health;

use axum::{Router, routing::get, routing::post};
use crate::state::AppState;

/// Build the API router with all routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health::health))
        .route("/bots", get(bots::list_bots).post(bots::create_bot))
        .route("/debates", get(debates::list_debates).post(debates::create_debate))
        .route("/debates/{id}", get(debates::get_debate))
        .with_state(state)
}
```

- [ ] **Step 5: Verify compilation (expect errors for missing bots/debates handlers)**

```bash
cargo check
```

- [ ] **Step 6: Commit**

```bash
git add src/api/
git commit -m "feat: API scaffold — auth extractor, health, DTOs, router"
```

---

## Task 5: Bot Registration Endpoints

**Files:**
- Create: `src/api/bots.rs`
- Create: `tests/api_bots_test.rs`
- Create: `tests/common/mod.rs`

- [ ] **Step 1: Write test helpers (tests/common/mod.rs)**

```rust
// tests/common/mod.rs
use axum::Router;
use sqlx::SqlitePool;
use bot_council::state::AppState;
use bot_council::config::{Settings, ServerConfig, DatabaseConfig, AuthConfig, HttpClientConfig};

/// Build a test app with in-memory SQLite.
pub async fn test_app() -> (Router, SqlitePool) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let settings = Settings {
        server: ServerConfig { host: "127.0.0.1".into(), port: 0 },
        database: DatabaseConfig { url: "sqlite::memory:".into() },
        auth: AuthConfig { admin_token: "".into() }, // no auth in tests
        http_client: HttpClientConfig {
            connect_timeout_secs: 5,
            request_timeout_secs: 30,
            max_retries: 0,
            retry_delay_secs: 1,
        },
    };

    let http_client = bot_council::bot_client::build_http_client(&settings.http_client);
    let state = AppState::new(pool.clone(), http_client, settings);
    let app = bot_council::api::router(state);
    (app, pool)
}
```

- [ ] **Step 2: Write failing test for POST /bots**

```rust
// tests/api_bots_test.rs
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use serde_json::{json, Value};

#[tokio::test]
async fn test_create_bot_returns_201() {
    let (app, _pool) = common::test_app().await;

    let body = json!({
        "name": "TestBot",
        "endpoint_url": "http://localhost:9999/debate",
        "token": "secret123"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/bots")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["name"], "TestBot");
    assert!(json["id"].is_string());
}

#[tokio::test]
async fn test_list_bots_returns_empty() {
    let (app, _pool) = common::test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/bots")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json.as_array().unwrap().is_empty());
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test --test api_bots_test
```

Expected: FAIL — `create_bot` and `list_bots` functions don't exist.

- [ ] **Step 4: Write api/bots.rs**

```rust
// src/api/bots.rs
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use sha2::{Sha256, Digest};
use crate::api::auth::BearerAuth;
use crate::api::dto::{CreateBotRequest, BotResponse};
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::types::BotId;

/// POST /bots — register a new bot.
pub async fn create_bot(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Json(req): Json<CreateBotRequest>,
) -> AppResult<(StatusCode, Json<BotResponse>)> {
    if req.name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    if req.endpoint_url.is_empty() {
        return Err(AppError::BadRequest("endpoint_url is required".into()));
    }
    if req.token.is_empty() {
        return Err(AppError::BadRequest("token is required".into()));
    }

    let id = BotId::new();
    let token_hash = hex::encode(Sha256::digest(req.token.as_bytes()));

    let row = queries::insert_bot(
        state.db(),
        id.as_str(),
        &req.name,
        &req.endpoint_url,
        &token_hash,
        req.model_family.as_deref(),
    )
    .await?;

    Ok((StatusCode::CREATED, Json(BotResponse {
        id: row.id,
        name: row.name,
        endpoint_url: row.endpoint_url,
        model_family: row.model_family,
        active: row.active,
        created_at: row.created_at,
    })))
}

/// GET /bots — list all active bots.
pub async fn list_bots(
    State(state): State<AppState>,
    _auth: BearerAuth,
) -> AppResult<Json<Vec<BotResponse>>> {
    let rows = queries::list_active_bots(state.db()).await?;
    let bots = rows.into_iter().map(|r| BotResponse {
        id: r.id,
        name: r.name,
        endpoint_url: r.endpoint_url,
        model_family: r.model_family,
        active: r.active,
        created_at: r.created_at,
    }).collect();
    Ok(Json(bots))
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test --test api_bots_test
```

Expected: PASS (both tests).

- [ ] **Step 6: Commit**

```bash
git add src/api/bots.rs tests/
git commit -m "feat: bot registration — POST /bots, GET /bots with tests"
```

---

## Task 6: Bot Client (outbound HTTP)

**Files:**
- Create: `src/bot_client/mod.rs`

- [ ] **Step 1: Write bot_client/mod.rs**

```rust
// src/bot_client/mod.rs
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{RetryTransientMiddleware, policies::ExponentialBackoff};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::config::HttpClientConfig;

/// Build the HTTP client with retry middleware.
pub fn build_http_client(config: &HttpClientConfig) -> ClientWithMiddleware {
    let retry_policy = ExponentialBackoff::builder()
        .retry_bounds(
            Duration::from_secs(config.retry_delay_secs),
            Duration::from_secs(config.retry_delay_secs * 4),
        )
        .build_with_max_retries(config.max_retries);

    let base = Client::builder()
        .timeout(Duration::from_secs(config.request_timeout_secs))
        .connect_timeout(Duration::from_secs(config.connect_timeout_secs))
        .build()
        .expect("failed to build reqwest client");

    ClientBuilder::new(base)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build()
}

/// Payload sent to a bot's /debate endpoint for position submission.
#[derive(Debug, Serialize)]
pub struct PositionRequest {
    pub session_id: String,
    pub round: i64,
    pub prompt: String,
}

/// Payload sent to a bot's /debate endpoint for scoring.
#[derive(Debug, Serialize)]
pub struct ScoringRequest {
    pub session_id: String,
    pub round: String, // "scoring"
    pub context: Vec<ScoringContext>,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScoringContext {
    pub pseudonym: String,
    pub response: String,
}

/// Response from a bot for position submission.
#[derive(Debug, Deserialize)]
pub struct PositionResponse {
    pub response: String,
}

/// Response from a bot for scoring.
#[derive(Debug, Deserialize)]
pub struct ScoringResponse {
    pub scores: Vec<ScoreEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ScoreEntry {
    pub pseudonym: String,
    pub reasoning_quality: i64,
    pub factual_grounding: i64,
    pub overall: i64,
    pub reasoning: String,
}

/// Send a position request to a bot.
pub async fn send_position_request(
    client: &ClientWithMiddleware,
    endpoint_url: &str,
    token: &str,
    request: &PositionRequest,
) -> Result<PositionResponse, String> {
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

    resp.json::<PositionResponse>()
        .await
        .map_err(|e| format!("invalid response body: {e}"))
}

/// Send a scoring request to a bot.
pub async fn send_scoring_request(
    client: &ClientWithMiddleware,
    endpoint_url: &str,
    token: &str,
    request: &ScoringRequest,
) -> Result<ScoringResponse, String> {
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

    resp.json::<ScoringResponse>()
        .await
        .map_err(|e| format!("invalid response body: {e}"))
}
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/bot_client/
git commit -m "feat: bot client — outbound HTTP with retry middleware"
```

---

## Task 7: Orchestrator (debate lifecycle)

**Files:**
- Create: `src/orchestrator/mod.rs`
- Create: `src/orchestrator/anonymiser.rs`

- [ ] **Step 1: Write orchestrator/anonymiser.rs**

```rust
// src/orchestrator/anonymiser.rs

/// Pseudonym list used for anonymisation. Stable order per debate.
const PSEUDONYMS: &[&str] = &[
    "Agent A", "Agent B", "Agent C", "Agent D", "Agent E",
    "Agent F", "Agent G", "Agent H", "Agent I", "Agent J",
];

/// Assign a pseudonym to a bot based on its index in the debate.
pub fn assign_pseudonym(index: usize) -> String {
    if index < PSEUDONYMS.len() {
        PSEUDONYMS[index].to_string()
    } else {
        format!("Agent {}", index + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assign_pseudonym() {
        assert_eq!(assign_pseudonym(0), "Agent A");
        assert_eq!(assign_pseudonym(4), "Agent E");
    }

    #[test]
    fn test_assign_pseudonym_overflow() {
        assert_eq!(assign_pseudonym(10), "Agent 11");
    }
}
```

- [ ] **Step 2: Write orchestrator/mod.rs**

```rust
// src/orchestrator/mod.rs
pub mod anonymiser;

use sqlx::SqlitePool;
use reqwest_middleware::ClientWithMiddleware;
use crate::bot_client::{
    self, PositionRequest, ScoringRequest, ScoringContext, ScoreEntry,
};
use crate::db::{models::BotRow, queries};
use crate::types::DebateId;

/// Result of a completed debate.
pub struct DebateResult {
    pub debate_id: String,
    pub rankings: Vec<RankedEntry>,
}

pub struct RankedEntry {
    pub pseudonym: String,
    pub avg_reasoning_quality: f64,
    pub avg_factual_grounding: f64,
    pub avg_overall: f64,
    pub total_scores: usize,
}

/// Run a single-shot debate: dispatch topic, collect responses, score, aggregate.
pub async fn run_debate(
    pool: &SqlitePool,
    client: &ClientWithMiddleware,
    debate_id: &DebateId,
    topic: &str,
    bots: &[BotRow],
    bot_tokens: &std::collections::HashMap<String, String>,
) -> Result<DebateResult, String> {
    let debate_id_str = debate_id.as_str();

    // Update status to dispatching
    queries::update_debate_status(pool, debate_id_str, "dispatching")
        .await
        .map_err(|e| format!("db error: {e}"))?;

    // --- Step 1: Dispatch topic to all bots concurrently ---
    let position_futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = debate_id_str.to_string();
        let topic = topic.to_string();

        async move {
            let req = PositionRequest {
                session_id,
                round: 0,
                prompt: format!(
                    "You are participating in a structured debate.\nTopic: {}\n\nState your position. Be substantive and specific.",
                    topic
                ),
            };

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(300),
                bot_client::send_position_request(&client, &endpoint, &token, &req),
            )
            .await;

            match result {
                Ok(Ok(resp)) => (bot.id.clone(), Some(resp.response)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot.id, error = %e, "bot position request failed");
                    (bot.id.clone(), None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot.id, "bot position request timed out");
                    (bot.id.clone(), None)
                }
            }
        }
    }).collect();

    let position_results = futures::future::join_all(position_futures).await;

    // Store responses and build anonymised context
    let mut anonymised: Vec<ScoringContext> = Vec::new();
    let debate_bots = queries::get_debate_bots(pool, debate_id_str)
        .await
        .map_err(|e| format!("db error: {e}"))?;

    for (bot_id, response_opt) in &position_results {
        let pseudonym = debate_bots
            .iter()
            .find(|db| db.bot_id == *bot_id)
            .map(|db| db.pseudonym.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        let (response_json, abstained) = match response_opt {
            Some(text) => (text.clone(), false),
            None => ("(abstained)".to_string(), true),
        };

        let resp_id = uuid::Uuid::new_v4().to_string();
        queries::insert_response(pool, &resp_id, debate_id_str, 0, bot_id, &response_json, abstained)
            .await
            .map_err(|e| format!("db error: {e}"))?;

        if !abstained {
            anonymised.push(ScoringContext {
                pseudonym: pseudonym.clone(),
                response: response_json,
            });
        }
    }

    // Check quorum (minimum 3 non-abstained)
    if anonymised.len() < 3 {
        queries::update_debate_status(pool, debate_id_str, "failed")
            .await
            .map_err(|e| format!("db error: {e}"))?;
        return Err(format!("quorum not met: only {} bots responded", anonymised.len()));
    }

    // --- Step 2: Send scoring requests to all bots concurrently ---
    queries::update_debate_status(pool, debate_id_str, "scoring")
        .await
        .map_err(|e| format!("db error: {e}"))?;

    let scoring_futures: Vec<_> = bots.iter().map(|bot| {
        let client = client.clone();
        let endpoint = bot.endpoint_url.clone();
        let token = bot_tokens.get(&bot.id).cloned().unwrap_or_default();
        let session_id = debate_id_str.to_string();
        let own_pseudonym = debate_bots
            .iter()
            .find(|db| db.bot_id == bot.id)
            .map(|db| db.pseudonym.clone())
            .unwrap_or_default();

        // Filter out this bot's own response
        let context: Vec<ScoringContext> = anonymised
            .iter()
            .filter(|c| c.pseudonym != own_pseudonym)
            .cloned()
            .collect();

        async move {
            let req = ScoringRequest {
                session_id,
                round: "scoring".to_string(),
                context,
                prompt: "Score each argument 0-10 on reasoning_quality and factual_grounding. Return JSON with a scores array.".to_string(),
            };

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(300),
                bot_client::send_scoring_request(&client, &endpoint, &token, &req),
            )
            .await;

            match result {
                Ok(Ok(resp)) => (bot.id.clone(), Some(resp.scores)),
                Ok(Err(e)) => {
                    tracing::warn!(bot_id = %bot.id, error = %e, "bot scoring request failed");
                    (bot.id.clone(), None)
                }
                Err(_) => {
                    tracing::warn!(bot_id = %bot.id, "bot scoring request timed out");
                    (bot.id.clone(), None)
                }
            }
        }
    }).collect();

    let scoring_results = futures::future::join_all(scoring_futures).await;

    // Store scores
    for (scorer_bot_id, scores_opt) in &scoring_results {
        if let Some(scores) = scores_opt {
            for score in scores {
                let score_id = uuid::Uuid::new_v4().to_string();
                // intentional: ignore insert errors for individual scores — log and continue
                if let Err(e) = queries::insert_peer_score(
                    pool,
                    &score_id,
                    debate_id_str,
                    scorer_bot_id,
                    &score.pseudonym,
                    score.reasoning_quality,
                    score.factual_grounding,
                    score.overall,
                    &score.reasoning,
                ).await {
                    tracing::warn!(
                        scorer = %scorer_bot_id,
                        target = %score.pseudonym,
                        error = %e,
                        "failed to store peer score"
                    );
                }
            }
        }
    }

    // --- Step 3: Aggregate scores into rankings ---
    let all_scores = queries::get_peer_scores(pool, debate_id_str)
        .await
        .map_err(|e| format!("db error: {e}"))?;

    let pseudonyms: Vec<String> = anonymised.iter().map(|c| c.pseudonym.clone()).collect();
    let mut rankings: Vec<RankedEntry> = pseudonyms
        .iter()
        .map(|p| {
            let scores: Vec<&crate::db::models::PeerScoreRow> = all_scores
                .iter()
                .filter(|s| s.target_pseudonym == *p)
                .collect();
            let count = scores.len();
            if count == 0 {
                return RankedEntry {
                    pseudonym: p.clone(),
                    avg_reasoning_quality: 0.0,
                    avg_factual_grounding: 0.0,
                    avg_overall: 0.0,
                    total_scores: 0,
                };
            }
            RankedEntry {
                pseudonym: p.clone(),
                avg_reasoning_quality: scores.iter().map(|s| s.reasoning_quality as f64).sum::<f64>() / count as f64,
                avg_factual_grounding: scores.iter().map(|s| s.factual_grounding as f64).sum::<f64>() / count as f64,
                avg_overall: scores.iter().map(|s| s.overall as f64).sum::<f64>() / count as f64,
                total_scores: count,
            }
        })
        .collect();

    // Sort by avg_overall descending
    rankings.sort_by(|a, b| b.avg_overall.partial_cmp(&a.avg_overall).unwrap_or(std::cmp::Ordering::Equal));

    // Mark complete
    queries::update_debate_status(pool, debate_id_str, "complete")
        .await
        .map_err(|e| format!("db error: {e}"))?;

    Ok(DebateResult {
        debate_id: debate_id_str.to_string(),
        rankings,
    })
}
```

- [ ] **Step 3: Add `futures` dependency to Cargo.toml**

Add under `[dependencies]`:
```toml
futures = "0.3"
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check
```

- [ ] **Step 5: Commit**

```bash
git add src/orchestrator/ Cargo.toml Cargo.lock
git commit -m "feat: orchestrator — debate lifecycle, anonymiser, score aggregation"
```

---

## Task 8: Debate Endpoints

**Files:**
- Create: `src/api/debates.rs`
- Create: `tests/api_debates_test.rs`

- [ ] **Step 1: Write api/debates.rs**

```rust
// src/api/debates.rs
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use crate::api::auth::BearerAuth;
use crate::api::dto::*;
use crate::db::queries;
use crate::error::{AppError, AppResult};
use crate::orchestrator;
use crate::orchestrator::anonymiser;
use crate::state::AppState;
use crate::types::{DebateId, DebateStatus};

/// POST /debates — create and run a debate.
pub async fn create_debate(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Json(req): Json<CreateDebateRequest>,
) -> AppResult<(StatusCode, Json<DebateResponse>)> {
    if req.topic.is_empty() {
        return Err(AppError::BadRequest("topic is required".into()));
    }

    // Resolve bots
    let bots = match &req.bot_ids {
        Some(ids) if !ids.is_empty() => {
            queries::get_bots_by_ids(state.db(), ids).await?
        }
        _ => {
            queries::list_active_bots(state.db()).await?
        }
    };

    if bots.len() < 3 {
        return Err(AppError::BadRequest(
            format!("need at least 3 bots, found {}", bots.len()),
        ));
    }

    // Create debate
    let debate_id = DebateId::new();
    queries::insert_debate(state.db(), debate_id.as_str(), &req.topic).await?;

    // Assign pseudonyms and register participation
    // NOTE: We need bot tokens for outbound calls. In Phase 0, tokens are stored hashed —
    // we cannot recover them. The bot_client sends the token from a lookup.
    // For Phase 0, we store the raw token in config or pass it at registration.
    // Workaround: store tokens in a separate in-memory map, or accept that Phase 0
    // uses a fixed test token. For now, we send an empty bearer (bots should accept).
    let mut bot_tokens = std::collections::HashMap::new();
    for (i, bot) in bots.iter().enumerate() {
        let pseudonym = anonymiser::assign_pseudonym(i);
        queries::insert_debate_bot(
            state.db(), debate_id.as_str(), &bot.id, &pseudonym
        ).await?;
        // Phase 0: token is empty — bots should handle auth or accept empty bearer
        bot_tokens.insert(bot.id.clone(), String::new());
    }

    // Spawn the debate as a background task
    let pool = state.db().clone();
    let client = state.http_client().clone();
    let topic = req.topic.clone();
    let debate_id_clone = debate_id.clone();
    let bots_clone = bots.clone();

    tokio::spawn(async move {
        match orchestrator::run_debate(
            &pool, &client, &debate_id_clone, &topic, &bots_clone, &bot_tokens,
        ).await {
            Ok(result) => {
                tracing::info!(
                    debate_id = %result.debate_id,
                    "debate completed with {} ranked arguments",
                    result.rankings.len()
                );
            }
            Err(e) => {
                tracing::error!(debate_id = %debate_id_clone, error = %e, "debate failed");
            }
        }
    });

    // Return immediately with created status
    let debate_bots = queries::get_debate_bots(state.db(), debate_id.as_str()).await?;
    let bot_infos: Vec<DebateBotInfo> = debate_bots.iter().map(|db| {
        let bot = bots.iter().find(|b| b.id == db.bot_id);
        DebateBotInfo {
            bot_id: db.bot_id.clone(),
            bot_name: bot.map(|b| b.name.clone()).unwrap_or_default(),
            pseudonym: db.pseudonym.clone(),
        }
    }).collect();

    Ok((StatusCode::CREATED, Json(DebateResponse {
        id: debate_id.to_string(),
        topic: req.topic,
        status: DebateStatus::Created.as_str().to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        completed_at: None,
        bots: bot_infos,
        results: None,
    })))
}

/// GET /debates — list debates.
pub async fn list_debates(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Query(params): Query<ListDebatesQuery>,
) -> AppResult<Json<Vec<DebateResponse>>> {
    let limit = params.limit.unwrap_or(20);
    let rows = queries::list_debates(state.db(), params.status.as_deref(), limit).await?;

    let mut debates = Vec::new();
    for row in rows {
        let debate_bots = queries::get_debate_bots(state.db(), &row.id).await?;
        let all_bots = queries::list_active_bots(state.db()).await?;
        let bot_infos: Vec<DebateBotInfo> = debate_bots.iter().map(|db| {
            let bot = all_bots.iter().find(|b| b.id == db.bot_id);
            DebateBotInfo {
                bot_id: db.bot_id.clone(),
                bot_name: bot.map(|b| b.name.clone()).unwrap_or_default(),
                pseudonym: db.pseudonym.clone(),
            }
        }).collect();

        debates.push(DebateResponse {
            id: row.id,
            topic: row.topic,
            status: row.status,
            created_at: row.created_at,
            completed_at: row.completed_at,
            bots: bot_infos,
            results: None,
        });
    }
    Ok(Json(debates))
}

/// GET /debates/{id} — get debate detail with results if complete.
pub async fn get_debate(
    State(state): State<AppState>,
    _auth: BearerAuth,
    Path(id): Path<String>,
) -> AppResult<Json<DebateResponse>> {
    let debate = queries::get_debate(state.db(), &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("debate {id} not found")))?;

    let debate_bots = queries::get_debate_bots(state.db(), &id).await?;
    let all_bots = queries::list_active_bots(state.db()).await?;
    let bot_infos: Vec<DebateBotInfo> = debate_bots.iter().map(|db| {
        let bot = all_bots.iter().find(|b| b.id == db.bot_id);
        DebateBotInfo {
            bot_id: db.bot_id.clone(),
            bot_name: bot.map(|b| b.name.clone()).unwrap_or_default(),
            pseudonym: db.pseudonym.clone(),
        }
    }).collect();

    // If complete, include results
    let results = if debate.status == "complete" {
        let responses = queries::get_responses(state.db(), &id, 0).await?;
        let scores = queries::get_peer_scores(state.db(), &id).await?;

        let anon_responses: Vec<AnonymisedResponse> = responses.iter().map(|r| {
            let pseudonym = debate_bots
                .iter()
                .find(|db| db.bot_id == r.bot_id)
                .map(|db| db.pseudonym.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            AnonymisedResponse {
                pseudonym,
                response: r.response_json.clone(),
                abstained: r.abstained,
            }
        }).collect();

        let pseudonyms: Vec<String> = debate_bots.iter().map(|db| db.pseudonym.clone()).collect();
        let mut rankings: Vec<RankedArgument> = pseudonyms.iter().map(|p| {
            let s: Vec<_> = scores.iter().filter(|s| s.target_pseudonym == *p).collect();
            let count = s.len();
            if count == 0 {
                return RankedArgument {
                    pseudonym: p.clone(),
                    avg_reasoning_quality: 0.0,
                    avg_factual_grounding: 0.0,
                    avg_overall: 0.0,
                    total_scores: 0,
                };
            }
            RankedArgument {
                pseudonym: p.clone(),
                avg_reasoning_quality: s.iter().map(|x| x.reasoning_quality as f64).sum::<f64>() / count as f64,
                avg_factual_grounding: s.iter().map(|x| x.factual_grounding as f64).sum::<f64>() / count as f64,
                avg_overall: s.iter().map(|x| x.overall as f64).sum::<f64>() / count as f64,
                total_scores: count,
            }
        }).collect();

        rankings.sort_by(|a, b| b.avg_overall.partial_cmp(&a.avg_overall).unwrap_or(std::cmp::Ordering::Equal));

        Some(DebateResults { responses: anon_responses, rankings })
    } else {
        None
    };

    Ok(Json(DebateResponse {
        id: debate.id,
        topic: debate.topic,
        status: debate.status,
        created_at: debate.created_at,
        completed_at: debate.completed_at,
        bots: bot_infos,
        results,
    }))
}
```

- [ ] **Step 2: Write integration test for debate creation**

```rust
// tests/api_debates_test.rs
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use serde_json::{json, Value};

async fn seed_bots(app: &mut axum::Router) -> Vec<String> {
    let mut ids = Vec::new();
    for i in 0..3 {
        let body = json!({
            "name": format!("Bot{}", i),
            "endpoint_url": format!("http://localhost:999{}/debate", i),
            "token": format!("token{}", i)
        });
        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/bots")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();
        ids.push(json["id"].as_str().unwrap().to_string());
    }
    ids
}

#[tokio::test]
async fn test_create_debate_returns_201() {
    let (mut app, _pool) = common::test_app().await;
    let bot_ids = seed_bots(&mut app).await;

    let body = json!({
        "topic": "Should AI-generated evidence be admissible in court?",
        "bot_ids": bot_ids
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/debates")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["topic"], "Should AI-generated evidence be admissible in court?");
    assert_eq!(json["bots"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn test_create_debate_rejects_insufficient_bots() {
    let (app, _pool) = common::test_app().await;

    let body = json!({
        "topic": "Test topic",
        "bot_ids": []
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/debates")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_get_debate_not_found() {
    let (app, _pool) = common::test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/debates/nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test --test api_debates_test
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/api/debates.rs tests/api_debates_test.rs
git commit -m "feat: debate endpoints — create, list, get with results"
```

---

## Task 9: Health Test

**Files:**
- Create: `tests/api_health_test.rs`

- [ ] **Step 1: Write health test**

```rust
// tests/api_health_test.rs
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use serde_json::Value;

#[tokio::test]
async fn test_health_returns_ok() {
    let (app, _pool) = common::test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}
```

- [ ] **Step 2: Run all tests**

```bash
cargo test
```

Expected: ALL PASS.

- [ ] **Step 3: Commit**

```bash
git add tests/api_health_test.rs
git commit -m "test: health endpoint integration test"
```

---

## Task 10: Reference Bot Endpoints

**Files:**
- Create: `reference/debate-endpoint-node.js`
- Create: `reference/debate-endpoint-python.py`

- [ ] **Step 1: Write Node.js reference endpoint**

```javascript
// reference/debate-endpoint-node.js
//
// Minimal /debate endpoint for testing the Bot Council harness.
// Run: node debate-endpoint-node.js [port]
// Default port: 9000
//
// This echoes a fixed position for round 0 and fixed scores for scoring.
// Replace the response logic with your bot's actual LLM calls.

const http = require("http");
const PORT = parseInt(process.argv[2] || "9000", 10);

function readBody(req) {
  return new Promise((resolve) => {
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => resolve(JSON.parse(Buffer.concat(chunks).toString())));
  });
}

const server = http.createServer(async (req, res) => {
  if (req.method === "POST" && req.url === "/debate") {
    const body = await readBody(req);
    let result;

    if (body.round === 0 || body.round === "0") {
      // Position submission
      result = {
        response: `This is my position on: ${body.prompt.substring(0, 100)}. I believe the key considerations are fairness, precedent, and practical enforceability.`,
      };
    } else if (body.round === "scoring") {
      // Scoring
      result = {
        scores: (body.context || []).map((entry) => ({
          pseudonym: entry.pseudonym,
          reasoning_quality: Math.floor(Math.random() * 4) + 5,
          factual_grounding: Math.floor(Math.random() * 4) + 5,
          overall: Math.floor(Math.random() * 4) + 5,
          reasoning: `${entry.pseudonym} presents a structured argument with clear reasoning.`,
        })),
      };
    } else {
      result = { error: "unknown round" };
    }

    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify(result));
  } else {
    res.writeHead(404);
    res.end("Not found");
  }
});

server.listen(PORT, () => console.log(`Reference bot listening on port ${PORT}`));
```

- [ ] **Step 2: Write Python reference endpoint**

```python
# reference/debate-endpoint-python.py
#
# Minimal /debate endpoint for testing the Bot Council harness.
# Run: python debate-endpoint-python.py [port]
# Default port: 9000
#
# Requires: pip install flask
# Replace response logic with your bot's actual LLM calls.

import json
import random
import sys
from http.server import HTTPServer, BaseHTTPRequestHandler


class DebateHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/debate":
            self.send_response(404)
            self.end_headers()
            return

        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length))

        if body.get("round") in (0, "0"):
            result = {
                "response": (
                    f"My position on this topic: {body.get('prompt', '')[:100]}. "
                    "The critical factors are evidence quality, procedural fairness, "
                    "and alignment with existing legal frameworks."
                ),
            }
        elif body.get("round") == "scoring":
            result = {
                "scores": [
                    {
                        "pseudonym": entry["pseudonym"],
                        "reasoning_quality": random.randint(5, 9),
                        "factual_grounding": random.randint(5, 9),
                        "overall": random.randint(5, 9),
                        "reasoning": f"{entry['pseudonym']} provides a well-structured argument.",
                    }
                    for entry in body.get("context", [])
                ],
            }
        else:
            result = {"error": "unknown round"}

        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(result).encode())

    def log_message(self, fmt, *args):
        print(f"[{self.log_date_time_string()}] {fmt % args}")


if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 9000
    server = HTTPServer(("0.0.0.0", port), DebateHandler)
    print(f"Reference bot listening on port {port}")
    server.serve_forever()
```

- [ ] **Step 3: Commit**

```bash
git add reference/
git commit -m "feat: reference /debate endpoints for Node.js and Python"
```

---

## Task 11: End-to-End Smoke Test

**Files:**
- Create: `tests/e2e_smoke_test.rs` (optional — manual test is acceptable for Phase 0)

- [ ] **Step 1: Start 3 reference bots in separate terminals**

```bash
node reference/debate-endpoint-node.js 9001 &
node reference/debate-endpoint-node.js 9002 &
node reference/debate-endpoint-node.js 9003 &
```

- [ ] **Step 2: Start the harness**

```bash
cargo run
```

Expected: "Bot Council listening on 0.0.0.0:3100"

- [ ] **Step 3: Register 3 bots**

```bash
curl -s -X POST http://localhost:3100/bots -H "Content-Type: application/json" \
  -d '{"name":"Bot1","endpoint_url":"http://localhost:9001/debate","token":"t1"}' | jq .

curl -s -X POST http://localhost:3100/bots -H "Content-Type: application/json" \
  -d '{"name":"Bot2","endpoint_url":"http://localhost:9002/debate","token":"t2"}' | jq .

curl -s -X POST http://localhost:3100/bots -H "Content-Type: application/json" \
  -d '{"name":"Bot3","endpoint_url":"http://localhost:9003/debate","token":"t3"}' | jq .
```

- [ ] **Step 4: Create a debate**

```bash
curl -s -X POST http://localhost:3100/debates -H "Content-Type: application/json" \
  -d '{"topic":"Should AI-generated evidence be admissible in court?"}' | jq .
```

Expected: 201 response with debate ID, 3 bots listed.

- [ ] **Step 5: Wait for completion, then fetch results**

```bash
# Wait a few seconds for the background task to complete
sleep 5

# Get debate ID from the create response, then:
curl -s http://localhost:3100/debates/<DEBATE_ID> | jq .
```

Expected: status "complete", results with anonymised responses and ranked arguments.

- [ ] **Step 6: Verify health**

```bash
curl -s http://localhost:3100/health | jq .
```

Expected: `{"status":"ok"}`

- [ ] **Step 7: Kill reference bots and commit any fixes**

```bash
kill %1 %2 %3
git add -A && git commit -m "chore: Phase 0 complete — end-to-end verified"
```

---

## Task 12: CLAUDE.md

**Files:**
- Create: `CLAUDE.md`

- [ ] **Step 1: Write project CLAUDE.md**

```markdown
# CLAUDE.md — LQ Bot Council Harness

## Quick Reference

| Key | Value |
|-----|-------|
| **Language** | Rust 2024 edition |
| **Framework** | Axum 0.8, Tokio |
| **Database** | SQLite via sqlx 0.8 |
| **Port** | 3100 |
| **Config** | config/default.toml + APP__* env vars |
| **Run** | `cargo run` |
| **Test** | `cargo test` |
| **Spec** | `docs/superpowers/specs/2026-04-15-bot-council-harness-design.md` |

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

## Current Phase: 0 (Single-Shot MVP)

Phase 0 supports: bot registration, single-shot debates, anonymisation,
peer scoring, ranked results. No rounds, roles, or LLM analysis.
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: add project CLAUDE.md with coding standards"
```
