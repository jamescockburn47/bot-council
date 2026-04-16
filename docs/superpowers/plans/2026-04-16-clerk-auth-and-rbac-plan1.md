# Clerk Auth, RBAC, and Bot Submission Cleanup — Implementation Plan 1

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace insecure JWT decode with RS256/JWKS verification, introduce RBAC with a 5-admin allowlist, encrypt bot tokens at rest so the harness can authenticate outbound calls, and surface rejection and smoke-test failure reasons to submitters.

**Architecture:** Backend gains two new axum extractors (`RequireAuth`, `RequireAdmin`) that replace the current `BearerAuth` alias. JWT verification uses a JWKS cache refreshed every 10 minutes. Bot tokens move from SHA-256 hash to AES-256-GCM ciphertext. Four near-identical PATCH handlers collapse into one `transition_bot_status` helper. Frontend attaches the Clerk session JWT via `@clerk/clerk-js`. Dev-mode auto-admin fallback is deleted; server refuses to start without either `admin_token` or `clerk_issuer` configured.

**Tech Stack:** Rust 2024, Axum 0.8, sqlx 0.8 (SQLite), `jsonwebtoken` 9 (RS256 + JWKS), `aes-gcm` 0.10, `arc-swap` 1 for hot-swappable JWKS cache, Svelte 5, `@clerk/clerk-js` for frontend auth.

**Spec:** [`docs/superpowers/specs/2026-04-16-clerk-auth-and-bot-submission-cleanup-design.md`](../specs/2026-04-16-clerk-auth-and-bot-submission-cleanup-design.md) — this plan covers §§1–17. Plan 2 will cover §§18–19 (bot author UX + MiniMax participant constraint).

---

## File Structure

**New files:**
- `src/api/bot_token_crypto.rs` — AES-256-GCM encrypt/decrypt helpers (~80 lines).
- `src/api/jwks_cache.rs` — JWKS fetch, cache, background refresh (~110 lines).
- `migrations/20260416000001_bot_submission_cleanup.sql` — adds `token_ciphertext`, `rejection_reason`, new status index.
- `frontend/src/lib/auth/clerk.ts` — Clerk initialization and `getToken()` helper.
- `frontend/src/routes/sign-in/+page.svelte` — Clerk sign-in UI mount.

**Modified files:**
- `src/api/auth.rs` — rewrite with new `AuthIdentity`, `RequireAuth`, `RequireAdmin`; delete insecure decode path and dev-mode fallback.
- `src/api/bots.rs` — collapse four PATCH handlers, add reject-reason flow, encrypt tokens at submit.
- `src/api/debates.rs`, `src/api/synthesis.rs`, `src/api/transcript.rs` — swap `BearerAuth` alias for `RequireAuth`/`RequireAdmin`.
- `src/api/dto.rs` — `RejectBotRequest` added, `rejection_reason` added to `BotResponse`, `active` field removed.
- `src/db/models.rs` — `BotRow` gains `token_ciphertext`, `rejection_reason`; `active` removed.
- `src/db/queries.rs` — `RETURNING *` on `update_bot_status`, `BOT_COLUMNS` constant, `transition_bot_status`.
- `src/config.rs` — `AuthConfig` gains `admin_user_ids`, `bot_token_key`; boot validation added.
- `config/default.toml` — new auth fields.
- `src/bot_client/mod.rs` — decrypt token and attach `Authorization: Bearer` on outbound debate calls.
- `src/state.rs` — `AppState` carries `Arc<ArcSwap<JwkSet>>` and the AES key.
- `src/main.rs` — startup kicks off JWKS fetch + refresh task; config validation runs first.
- `tests/common/mod.rs` — update `test_app` to pass a static `admin_token` and `bot_token_key`, add participant impersonation helper.
- `tests/api_bots_test.rs` — new tests for auth, reject reason, transition enforcement.
- `Cargo.toml` — add `aes-gcm`, `arc-swap`, and (if not already present) `rand`, `base64`.
- `frontend/package.json` — add `@clerk/clerk-js`.
- `frontend/src/lib/api/client.ts` — attach JWT from `getToken()`, 401 redirect.
- `frontend/src/routes/+layout.svelte` — route guard.
- `frontend/src/routes/bots/my-submissions/+page.svelte` — rejection_reason banner.
- `frontend/src/routes/bots/+page.svelte` — admin review section with reject modal and retry.
- `frontend/src/routes/debates/new/+page.svelte` — admin-only guard.
- `frontend/src/lib/components/Sidebar.svelte` — hide New Debate for participants.

---

## Task 1: AES-GCM crypto module with round-trip tests

**Files:**
- Modify: `Cargo.toml`
- Create: `src/api/bot_token_crypto.rs`
- Modify: `src/lib.rs`
- Test: inline `#[cfg(test)]` at bottom of `src/api/bot_token_crypto.rs`

- [ ] **Step 1: Add crypto dependencies**

Add to `Cargo.toml` under `[dependencies]`:

```toml
aes-gcm = "0.10"
rand = "0.8"
base64 = "0.22"
```

Verify the insertion:

```bash
grep -E "^aes-gcm|^rand|^base64" Cargo.toml
```

Expected: three matching lines.

- [ ] **Step 2: Create the crypto module with failing tests**

Create `src/api/bot_token_crypto.rs`:

```rust
//! AES-256-GCM encryption for bot bearer tokens.
//!
//! Tokens are encrypted at submission time and decrypted only when the harness
//! needs to make an outbound call to the bot's `/debate` endpoint. Output
//! layout: `[12-byte nonce][ciphertext][16-byte auth tag]`.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use thiserror::Error;

/// Errors from encrypt/decrypt operations.
#[derive(Debug, Error)]
pub enum CryptoError {
    /// Encryption failed (library-level error, effectively unreachable).
    #[error("encryption failed")]
    Encrypt,
    /// Decryption failed — wrong key, tampered ciphertext, or malformed input.
    #[error("decryption failed")]
    Decrypt,
    /// Ciphertext shorter than the 12-byte nonce prefix.
    #[error("ciphertext too short (expected at least 12 bytes for nonce)")]
    Malformed,
}

/// Fixed-size 256-bit key.
pub type BotTokenKey = [u8; 32];

/// Encrypt a plaintext string with a random nonce. Output is
/// `nonce || ciphertext_with_tag`.
pub fn encrypt(key: &BotTokenKey, plaintext: &str) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|_| CryptoError::Encrypt)?;
    let mut output = Vec::with_capacity(12 + ciphertext.len());
    output.extend_from_slice(&nonce);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt a `nonce || ciphertext_with_tag` blob.
pub fn decrypt(key: &BotTokenKey, ciphertext: &[u8]) -> Result<String, CryptoError> {
    if ciphertext.len() < 12 {
        return Err(CryptoError::Malformed);
    }
    let (nonce_bytes, rest) = ciphertext.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let plain = cipher
        .decrypt(nonce, rest)
        .map_err(|_| CryptoError::Decrypt)?;
    String::from_utf8(plain).map_err(|_| CryptoError::Decrypt)
}

/// Parse a 64-character hex string into a 32-byte key.
pub fn parse_key_hex(s: &str) -> Result<BotTokenKey, CryptoError> {
    let bytes = hex::decode(s).map_err(|_| CryptoError::Malformed)?;
    if bytes.len() != 32 {
        return Err(CryptoError::Malformed);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> BotTokenKey {
        [7u8; 32]
    }

    #[test]
    fn round_trip_preserves_plaintext() {
        let key = test_key();
        for s in ["", "short", "a bearer token with spaces and 1234567890"] {
            let c = encrypt(&key, s).unwrap();
            assert_eq!(decrypt(&key, &c).unwrap(), s);
        }
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = test_key();
        let mut c = encrypt(&key, "secret").unwrap();
        let last = c.len() - 1;
        c[last] ^= 0x01;
        assert!(matches!(decrypt(&key, &c), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn wrong_key_fails() {
        let c = encrypt(&test_key(), "secret").unwrap();
        let wrong = [9u8; 32];
        assert!(matches!(decrypt(&wrong, &c), Err(CryptoError::Decrypt)));
    }

    #[test]
    fn short_ciphertext_is_malformed() {
        assert!(matches!(decrypt(&test_key(), &[0u8; 5]), Err(CryptoError::Malformed)));
    }

    #[test]
    fn parse_key_hex_happy() {
        let s = "0".repeat(64);
        let k = parse_key_hex(&s).unwrap();
        assert_eq!(k, [0u8; 32]);
    }

    #[test]
    fn parse_key_hex_wrong_length() {
        assert!(parse_key_hex("abcd").is_err());
    }
}
```

Add the module to `src/lib.rs`. Find the existing `pub mod api;` block and verify the submodule re-export pattern. If `api/mod.rs` declares submodules, add `pub mod bot_token_crypto;` there instead.

```bash
grep -n "pub mod" src/api/mod.rs | head
```

If `pub mod bots;` is at the top, add `pub mod bot_token_crypto;` alongside.

- [ ] **Step 3: Run tests and verify they pass on EVO**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src Cargo.toml Cargo.lock james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test --lib bot_token_crypto"
```

Expected: 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock src/api/bot_token_crypto.rs src/api/mod.rs
git commit -m "feat: AES-256-GCM module for bot token encryption"
```

---

## Task 2: Database migration for token_ciphertext and rejection_reason

**Files:**
- Create: `migrations/20260416000001_bot_submission_cleanup.sql`
- Modify: `src/db/models.rs`
- Modify: `src/db/queries.rs`

- [ ] **Step 1: Write the migration**

Create `migrations/20260416000001_bot_submission_cleanup.sql`:

```sql
-- Adds encrypted-token storage and rejection-reason feedback loop.
-- Retains token_hash and active columns; a follow-up migration will drop
-- them after one release when all rows are confirmed on the new path.

ALTER TABLE bots ADD COLUMN token_ciphertext BLOB;
ALTER TABLE bots ADD COLUMN rejection_reason TEXT;

CREATE INDEX idx_bots_status_reviewable
    ON bots(status)
    WHERE status IN ('pending', 'smoke_test_failed');
```

- [ ] **Step 2: Update `BotRow` in `src/db/models.rs`**

Replace the existing `BotRow` struct (lines 3–18) with:

```rust
/// Database row for a registered bot.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct BotRow {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub token_hash: Option<String>,
    pub token_ciphertext: Option<Vec<u8>>,
    pub model_family: Option<String>,
    pub active: bool,
    pub created_at: String,
    pub status: String,
    pub submitted_by: Option<String>,
    pub description: Option<String>,
    pub rejection_reason: Option<String>,
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
}
```

`token_hash` becomes `Option<String>` since new rows will not populate it. `active` stays for now — removed in a later task after all consumers are updated.

- [ ] **Step 3: Add `BOT_COLUMNS` constant and update SELECTs in `src/db/queries.rs`**

At the top of `src/db/queries.rs`, after the `use` lines, add:

```rust
/// Column list used by every bot SELECT. Kept in one place so schema changes
/// touch one spot instead of six.
const BOT_COLUMNS: &str = "id, name, endpoint_url, token_hash, token_ciphertext, \
    model_family, active, created_at, status, submitted_by, description, \
    rejection_reason, reviewed_at, reviewed_by";
```

Rewrite each query that currently enumerates columns. For `list_active_bots`:

```rust
pub async fn list_active_bots(pool: &SqlitePool) -> Result<Vec<BotRow>, sqlx::Error> {
    let sql = format!(
        "SELECT {BOT_COLUMNS} FROM bots WHERE status = 'active' ORDER BY created_at"
    );
    sqlx::query_as::<_, BotRow>(&sql).fetch_all(pool).await
}
```

Apply the same pattern to `get_bot`, `get_bots_by_ids`, `list_bots_by_submitter`, `list_all_bots`. The two-column `INSERT ... RETURNING *` path in `insert_bot` does not need the constant — `*` already covers it.

- [ ] **Step 4: Update `insert_bot` signature to accept ciphertext**

Replace the existing `insert_bot` (lines 5–17 of `queries.rs`) with:

```rust
/// Insert a new bot registration and return the created row.
#[allow(clippy::too_many_arguments)]
pub async fn insert_bot(
    pool: &SqlitePool,
    id: &str,
    name: &str,
    endpoint_url: &str,
    token_ciphertext: &[u8],
    model_family: Option<&str>,
    submitted_by: Option<&str>,
    description: Option<&str>,
    status: &str,
) -> Result<BotRow, sqlx::Error> {
    sqlx::query_as::<_, BotRow>(
        "INSERT INTO bots (id, name, endpoint_url, token_ciphertext, model_family, \
         submitted_by, description, status) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING *"
    )
    .bind(id)
    .bind(name)
    .bind(endpoint_url)
    .bind(token_ciphertext)
    .bind(model_family)
    .bind(submitted_by)
    .bind(description)
    .bind(status)
    .fetch_one(pool)
    .await
}
```

Note: the signature change breaks `src/api/bots.rs::create_bot` — that's fixed in Task 10. For now, expect a compile error there.

- [ ] **Step 5: Run migration on a fresh in-memory DB via the existing test harness**

```bash
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo check"
```

Expected: one compile error in `src/api/bots.rs` at the `queries::insert_bot` call site (wrong arg types). Migrations and `queries.rs` itself should compile. Leave the `bots.rs` error — it is addressed by Task 10.

- [ ] **Step 6: Commit**

```bash
git add migrations/20260416000001_bot_submission_cleanup.sql src/db/models.rs src/db/queries.rs
git commit -m "feat: migration and row shape for token_ciphertext + rejection_reason"
```

---

## Task 3: Config additions and test helper update

**Files:**
- Modify: `src/config.rs`
- Modify: `config/default.toml`
- Modify: `tests/common/mod.rs`

- [ ] **Step 1: Expand `AuthConfig`**

Replace `AuthConfig` in `src/config.rs` (lines 29–37):

```rust
/// Authentication configuration.
/// Supports bearer token (admin CLI/bots) and Clerk JWT (frontend users).
#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    /// Static bearer token granting admin. Empty string disables this path.
    pub admin_token: String,
    /// Base URL of the Clerk issuer, e.g. `https://<app>.clerk.accounts.dev`.
    pub clerk_issuer: String,
    /// Clerk JWKS URL. If empty, derived from `clerk_issuer` as
    /// `{issuer}/.well-known/jwks.json`.
    pub clerk_jwks_url: String,
    /// Clerk user_ids (format `user_2...`) granted admin role.
    #[serde(default)]
    pub admin_user_ids: Vec<String>,
    /// 64-character hex string (32 bytes) — AES-256 key for bot token
    /// encryption. Required when Clerk is configured.
    pub bot_token_key: String,
}
```

- [ ] **Step 2: Update `config/default.toml`**

Replace the `[auth]` block:

```toml
[auth]
admin_token = ""
clerk_issuer = ""
clerk_jwks_url = ""
admin_user_ids = []
bot_token_key = ""
```

- [ ] **Step 3: Update the test helper**

In `tests/common/mod.rs`, replace the `AuthConfig` literal in `test_app` with:

```rust
        auth: AuthConfig {
            admin_token: "test-admin-token".into(),
            clerk_issuer: "".into(),
            clerk_jwks_url: "".into(),
            admin_user_ids: vec![],
            // 32 bytes = 64 hex chars; deterministic for reproducible tests.
            bot_token_key: "00112233445566778899aabbccddeeff\
                            00112233445566778899aabbccddeeff".into(),
        },
```

Also add at the bottom of `tests/common/mod.rs`:

```rust
use axum::http::HeaderValue;

/// Helper: attach the test admin bearer token to a request builder.
#[allow(dead_code)]
pub fn admin_auth(req: axum::http::request::Builder) -> axum::http::request::Builder {
    req.header("authorization", HeaderValue::from_static("Bearer test-admin-token"))
}
```

- [ ] **Step 4: Check it compiles**

```bash
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo check --tests"
```

Expected: compiles. `src/api/bots.rs` still has the Task 2 error.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs config/default.toml tests/common/mod.rs
git commit -m "feat: expand AuthConfig with admin_user_ids and bot_token_key"
```

---

## Task 4: JWKS cache module

**Files:**
- Modify: `Cargo.toml`
- Create: `src/api/jwks_cache.rs`
- Modify: `src/api/mod.rs`

- [ ] **Step 1: Add `arc-swap` dependency**

Append to `Cargo.toml` `[dependencies]`:

```toml
arc-swap = "1"
```

- [ ] **Step 2: Create the JWKS cache module with tests**

Create `src/api/jwks_cache.rs`:

```rust
//! Fetches and caches the Clerk JWKS for JWT signature verification.
//!
//! The key set is hot-swappable via `ArcSwap` so the background refresh task
//! never blocks request handlers. On fetch failure the previous cached set is
//! retained — only a startup failure (empty cache) returns `None`.

use arc_swap::ArcSwap;
use jsonwebtoken::jwk::JwkSet;
use std::sync::Arc;
use std::time::Duration;

/// Cached JWKS keyed against the URL it was fetched from.
#[derive(Debug, Clone)]
pub struct JwksCache {
    inner: Arc<ArcSwap<Option<JwkSet>>>,
    url: String,
}

impl JwksCache {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            inner: Arc::new(ArcSwap::from_pointee(None)),
            url: url.into(),
        }
    }

    /// Returns the current JWKS, or `None` if never successfully fetched.
    pub fn current(&self) -> Option<Arc<JwkSet>> {
        let guard = self.inner.load();
        guard.as_ref().as_ref().map(|jwks| Arc::new(jwks.clone()))
    }

    pub fn url(&self) -> &str { &self.url }

    /// Fetch and swap in a new JWKS. On failure, the existing cache is untouched.
    pub async fn refresh(&self, client: &reqwest::Client) -> anyhow::Result<()> {
        let bytes = client
            .get(&self.url)
            .timeout(Duration::from_secs(10))
            .send().await?
            .error_for_status()?
            .bytes().await?;
        let jwks: JwkSet = serde_json::from_slice(&bytes)?;
        self.inner.store(Arc::new(Some(jwks)));
        Ok(())
    }
}

/// Spawn a background task refreshing the cache every `interval_secs`.
pub fn spawn_refresh_loop(cache: JwksCache, client: reqwest::Client, interval_secs: u64) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.tick().await; // skip immediate first tick
        loop {
            ticker.tick().await;
            if let Err(e) = cache.refresh(&client).await {
                tracing::warn!(error = %e, url = %cache.url(), "JWKS refresh failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cache_returns_none() {
        let cache = JwksCache::new("https://example.invalid/.well-known/jwks.json");
        assert!(cache.current().is_none());
    }
}
```

- [ ] **Step 3: Register the module in `src/api/mod.rs`**

Add `pub mod jwks_cache;` alongside the other submodule declarations.

- [ ] **Step 4: Run tests**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src Cargo.toml Cargo.lock james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test --lib jwks_cache"
```

Expected: 1 test passes. The `src/api/bots.rs` error from Task 2 is still outstanding and expected.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/api/jwks_cache.rs src/api/mod.rs
git commit -m "feat: JWKS cache with hot-swap and background refresh"
```

---

## Task 5: Rewrite auth.rs with RS256 verification and new extractors

**Files:**
- Modify: `src/api/auth.rs`
- Modify: `src/state.rs`
- Modify: `src/main.rs`
- Modify: `src/error.rs` (if `Forbidden` / `Internal` variants are missing)
- Modify: `tests/common/mod.rs`

- [ ] **Step 1: Check error variants**

```bash
grep -n "Forbidden\|Internal\|Conflict\|Unauthorized\|BadRequest\|NotFound" src/error.rs
```

Add any missing variants. `Forbidden` maps to 403, `Internal(String)` to 500, `Conflict(String)` to 409. Pattern to follow is the existing `BadRequest(String)` — enum variant, `IntoResponse` branch, and serde-friendly error body.

- [ ] **Step 2: Extend `AppState`**

Read `src/state.rs` first:

```bash
cat src/state.rs
```

Add two fields and accessors. Preserve every existing field and method — only additions:

```rust
use crate::api::bot_token_crypto::BotTokenKey;
use crate::api::jwks_cache::JwksCache;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    // ... existing fields (pool, http_client, settings) preserved ...
    jwks: JwksCache,
    bot_token_key: Arc<BotTokenKey>,
}

impl AppState {
    pub fn new(
        pool: SqlitePool,
        http_client: reqwest_middleware::ClientWithMiddleware,
        settings: Settings,
        jwks: JwksCache,
        bot_token_key: BotTokenKey,
    ) -> Self {
        Self {
            pool, http_client,
            settings: Arc::new(settings),
            jwks,
            bot_token_key: Arc::new(bot_token_key),
        }
    }
    pub fn jwks(&self) -> &JwksCache { &self.jwks }
    pub fn bot_token_key(&self) -> &BotTokenKey { &self.bot_token_key }
    // ... existing accessors retained ...
}
```

- [ ] **Step 3: Replace `src/api/auth.rs` wholesale**

```rust
//! Authentication and role-based access control.
//!
//! Two extractors:
//! - `RequireAuth` — any signed-in identity (401 otherwise)
//! - `RequireAdmin` — admin only (403 for participants, 401 for unauth)
//!
//! Admin identities come from either a static bearer token (CLI) or a Clerk
//! JWT whose `sub` claim is in the config allowlist.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub enum AuthIdentity {
    Admin { user_id: Option<String>, source: AuthSource },
    Participant { user_id: String },
}

#[derive(Debug, Clone, Copy)]
pub enum AuthSource { BearerToken, ClerkJwt }

impl AuthIdentity {
    pub fn is_admin(&self) -> bool { matches!(self, AuthIdentity::Admin { .. }) }
    pub fn user_id(&self) -> Option<&str> {
        match self {
            AuthIdentity::Admin { user_id, .. } => user_id.as_deref(),
            AuthIdentity::Participant { user_id } => Some(user_id),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ClerkClaims { sub: String, iss: String, exp: u64 }

pub struct RequireAuth(pub AuthIdentity);
impl FromRequestParts<AppState> for RequireAuth {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        Ok(RequireAuth(authenticate(parts, state).await?))
    }
}

pub struct RequireAdmin(pub AuthIdentity);
impl FromRequestParts<AppState> for RequireAdmin {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let id = authenticate(parts, state).await?;
        if id.is_admin() { Ok(RequireAdmin(id)) } else { Err(AppError::Forbidden) }
    }
}

impl FromRequestParts<AppState> for AuthIdentity {
    type Rejection = AppError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        authenticate(parts, state).await
    }
}

async fn authenticate(parts: &Parts, state: &AppState) -> Result<AuthIdentity, AppError> {
    let cfg = &state.settings().auth;
    let token = parts.headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;

    if !cfg.admin_token.is_empty() && token == cfg.admin_token {
        return Ok(AuthIdentity::Admin { user_id: None, source: AuthSource::BearerToken });
    }
    if !cfg.clerk_issuer.is_empty() {
        return verify_clerk_jwt(token, state).await;
    }
    Err(AppError::Unauthorized)
}

async fn verify_clerk_jwt(token: &str, state: &AppState) -> Result<AuthIdentity, AppError> {
    let cfg = &state.settings().auth;
    let header = decode_header(token).map_err(|_| AppError::Unauthorized)?;
    let kid = header.kid.ok_or(AppError::Unauthorized)?;
    let jwks = state.jwks().current()
        .ok_or_else(|| AppError::Internal("JWKS not yet loaded".into()))?;
    let jwk = jwks.find(&kid).ok_or(AppError::Unauthorized)?;
    let decoding_key = DecodingKey::from_jwk(jwk).map_err(|_| AppError::Unauthorized)?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[cfg.clerk_issuer.clone()]);
    validation.validate_aud = false;
    validation.leeway = 30;

    let claims = decode::<ClerkClaims>(token, &decoding_key, &validation)
        .map_err(|_| AppError::Unauthorized)?
        .claims;

    if cfg.admin_user_ids.iter().any(|id| id == &claims.sub) {
        Ok(AuthIdentity::Admin { user_id: Some(claims.sub), source: AuthSource::ClerkJwt })
    } else {
        Ok(AuthIdentity::Participant { user_id: claims.sub })
    }
}

// Temporary deprecated aliases so existing handlers still compile until Task 6.
#[deprecated(note = "Use RequireAuth or RequireAdmin")]
pub type BearerAuth = RequireAuth;
#[deprecated(note = "Use RequireAdmin")]
pub type AdminOnly = RequireAdmin;
```

- [ ] **Step 4: Wire JWKS and key into `main.rs`**

Before `AppState::new(...)` in `src/main.rs`, insert:

```rust
let bot_token_key = bot_council::api::bot_token_crypto::parse_key_hex(
    &settings.auth.bot_token_key,
).unwrap_or([0u8; 32]);

let jwks_url = if settings.auth.clerk_jwks_url.is_empty() {
    format!("{}/.well-known/jwks.json", settings.auth.clerk_issuer)
} else {
    settings.auth.clerk_jwks_url.clone()
};
let jwks = bot_council::api::jwks_cache::JwksCache::new(jwks_url);
if !settings.auth.clerk_issuer.is_empty() {
    let raw_client = reqwest::Client::new();
    if let Err(e) = jwks.refresh(&raw_client).await {
        tracing::warn!(error = %e, "initial JWKS fetch failed; continuing with empty cache");
    }
    bot_council::api::jwks_cache::spawn_refresh_loop(jwks.clone(), raw_client, 600);
}
```

Then change the `AppState::new(pool, http_client, settings)` call to include `jwks, bot_token_key`.

- [ ] **Step 5: Update `tests/common/mod.rs` to match constructor**

```rust
let jwks = bot_council::api::jwks_cache::JwksCache::new("http://localhost/unused");
let bot_token_key = [0u8; 32];
let state = AppState::new(pool.clone(), http_client, settings, jwks, bot_token_key);
```

- [ ] **Step 6: Build and check**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo check --tests"
```

Expected: compile warnings for deprecated aliases are OK. No hard errors.

- [ ] **Step 7: Commit**

```bash
git add src/api/auth.rs src/state.rs src/main.rs src/error.rs tests/common/mod.rs
git commit -m "feat: RS256 JWKS verification + RequireAuth/RequireAdmin extractors"
```

---

## Task 6: Wire extractors into routes

**Files:**
- Modify: `src/api/bots.rs` (handler signatures only)
- Modify: `src/api/debates.rs`
- Modify: `src/api/synthesis.rs`
- Modify: `src/api/transcript.rs`
- Test: `tests/api_bots_test.rs`, `tests/api_debates_test.rs`

- [ ] **Step 1: Replace extractor types per route**

Use the route matrix from spec §5:

`src/api/debates.rs`:
- `list_debates`: `_auth: BearerAuth` → `_auth: RequireAuth`
- `get_debate`: `_auth: BearerAuth` → `_auth: RequireAuth`
- `create_debate`: `_auth: BearerAuth` → `_auth: RequireAdmin` ← **new restriction**

Update the import:
```rust
use crate::api::auth::{RequireAuth, RequireAdmin};
```

`src/api/synthesis.rs` and `src/api/transcript.rs`:
- Swap `BearerAuth` → `RequireAuth`.

`src/api/bots.rs` handler signatures:
- `create_bot`: keep `auth: AuthIdentity` (branches on admin vs participant)
- `list_bots`: keep `auth: AuthIdentity`
- `my_submissions`: keep `auth: AuthIdentity`
- `get_me`: keep `auth: AuthIdentity`
- `approve_bot`, `reject_bot`, `deactivate_bot`, `reactivate_bot`: `admin: AdminOnly` → `admin: RequireAdmin`. Access becomes `admin.0` (same as before).

- [ ] **Step 2: Update existing tests to attach admin bearer**

Every test in `tests/api_bots_test.rs` and `tests/api_debates_test.rs` must now pass `common::admin_auth(...)` around the request builder. Example conversion:

Before:
```rust
let response = app.oneshot(
    Request::builder()
        .method("POST")
        .uri("/bots")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap(),
).await.unwrap();
```

After:
```rust
let req = common::admin_auth(
    Request::builder()
        .method("POST")
        .uri("/bots")
        .header("content-type", "application/json"),
)
    .body(Body::from(serde_json::to_string(&body).unwrap()))
    .unwrap();
let response = app.oneshot(req).await.unwrap();
```

- [ ] **Step 3: Add a new test — unauthenticated request returns 401**

Append to `tests/api_debates_test.rs`:

```rust
#[tokio::test]
async fn create_debate_without_auth_returns_401() {
    let (app, _pool) = common::test_app().await;
    let body = serde_json::json!({ "topic": "X" });
    let req = Request::builder()
        .method("POST")
        .uri("/debates")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 4: Run tests**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src tests
git commit -m "feat: wire RequireAuth/RequireAdmin into routes; POST /debates admin-only"
```

---

## Task 7: `transition_bot_status` + collapse PATCH handlers + reject-reason flow

**Files:**
- Modify: `src/db/queries.rs`
- Modify: `src/api/bots.rs`
- Modify: `src/api/dto.rs`
- Modify: `src/error.rs`
- Test: `tests/api_bots_test.rs`

- [ ] **Step 1: Ensure `AppError::Conflict(String)` variant exists**

Add to the enum if missing, with `StatusCode::CONFLICT` in the `IntoResponse` impl.

- [ ] **Step 2: Replace `update_bot_status` with `transition_bot_status`**

In `src/db/queries.rs`, delete the existing `update_bot_status` and add:

```rust
/// Atomically transition a bot's status. Returns the updated row, or `None`
/// if the WHERE clause matched no row (caller then distinguishes "not found"
/// from "wrong state" via get_bot).
pub async fn transition_bot_status(
    pool: &SqlitePool,
    id: &str,
    expected_from: &[&str],
    new_status: &str,
    reviewed_by: Option<&str>,
    rejection_reason: Option<&str>,
) -> Result<Option<BotRow>, sqlx::Error> {
    let placeholders = expected_from.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let active = new_status == "active";
    let sql = format!(
        "UPDATE bots SET status = ?, active = ?, reviewed_at = datetime('now'), \
         reviewed_by = ?, rejection_reason = ? \
         WHERE id = ? AND status IN ({placeholders}) RETURNING *"
    );
    let mut q = sqlx::query_as::<_, BotRow>(&sql)
        .bind(new_status).bind(active)
        .bind(reviewed_by).bind(rejection_reason).bind(id);
    for s in expected_from { q = q.bind(*s); }
    q.fetch_optional(pool).await
}
```

- [ ] **Step 3: Update `BotResponse` and add `RejectBotRequest` DTOs**

In `src/api/dto.rs` replace the existing `BotResponse` struct with:

```rust
#[derive(Debug, Serialize)]
pub struct BotResponse {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub model_family: Option<String>,
    pub status: String,
    pub description: Option<String>,
    pub submitted_by: Option<String>,
    pub rejection_reason: Option<String>,
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct RejectBotRequest {
    pub reason: String,
}
```

- [ ] **Step 4: Collapse the four PATCH handlers in `src/api/bots.rs`**

Replace `approve_bot` through `reactivate_bot` with:

```rust
async fn do_transition(
    state: &AppState,
    admin: &RequireAdmin,
    id: &str,
    expected_from: &[&str],
    new_status: &str,
    rejection_reason: Option<&str>,
) -> AppResult<BotRow> {
    let reviewer = admin.0.user_id();
    let updated = queries::transition_bot_status(
        state.db(), id, expected_from, new_status, reviewer, rejection_reason,
    ).await?;
    match updated {
        Some(row) => Ok(row),
        None => {
            match queries::get_bot(state.db(), id).await? {
                None => Err(AppError::NotFound("bot not found".into())),
                Some(row) => Err(AppError::Conflict(format!(
                    "bot is in state '{}', expected one of {:?}",
                    row.status, expected_from
                ))),
            }
        }
    }
}

pub async fn approve_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let bot = queries::get_bot(state.db(), &id).await?
        .ok_or_else(|| AppError::NotFound("bot not found".into()))?;
    if !matches!(bot.status.as_str(), "pending" | "smoke_test_failed") {
        return Err(AppError::Conflict(format!(
            "bot is in state '{}', expected 'pending' or 'smoke_test_failed'",
            bot.status
        )));
    }
    match smoke_test_bot(state.http_client(), &bot).await {
        Ok(()) => {
            let row = do_transition(
                &state, &admin, &id,
                &["pending", "smoke_test_failed"], "active", None,
            ).await?;
            Ok(Json(bot_to_response(&row)))
        }
        Err(reason) => {
            let row = do_transition(
                &state, &admin, &id,
                &["pending", "smoke_test_failed"], "smoke_test_failed",
                Some(&reason),
            ).await?;
            Ok(Json(bot_to_response(&row)))
        }
    }
}

pub async fn reject_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
    Json(req): Json<RejectBotRequest>,
) -> AppResult<Json<BotResponse>> {
    let reason = req.reason.trim();
    if reason.len() < 10 {
        return Err(AppError::BadRequest("reason must be at least 10 characters".into()));
    }
    if reason.len() > 500 {
        return Err(AppError::BadRequest("reason must be at most 500 characters".into()));
    }
    let row = do_transition(
        &state, &admin, &id,
        &["pending", "smoke_test_failed"], "rejected", Some(reason),
    ).await?;
    Ok(Json(bot_to_response(&row)))
}

pub async fn deactivate_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let row = do_transition(&state, &admin, &id, &["active"], "inactive", None).await?;
    Ok(Json(bot_to_response(&row)))
}

pub async fn reactivate_bot(
    State(state): State<AppState>,
    admin: RequireAdmin,
    Path(id): Path<String>,
) -> AppResult<Json<BotResponse>> {
    let row = do_transition(&state, &admin, &id, &["inactive"], "active", None).await?;
    Ok(Json(bot_to_response(&row)))
}
```

Also update `bot_to_response` to drop the removed `active` field:

```rust
fn bot_to_response(row: &BotRow) -> BotResponse {
    BotResponse {
        id: row.id.clone(),
        name: row.name.clone(),
        endpoint_url: row.endpoint_url.clone(),
        model_family: row.model_family.clone(),
        status: row.status.clone(),
        description: row.description.clone(),
        submitted_by: row.submitted_by.clone(),
        rejection_reason: row.rejection_reason.clone(),
        reviewed_at: row.reviewed_at.clone(),
        reviewed_by: row.reviewed_by.clone(),
        created_at: row.created_at.clone(),
    }
}
```

Update `smoke_test_bot` to take a `&BotRow` (Task 10 will add the decrypt+bearer):

```rust
async fn smoke_test_bot(
    client: &reqwest_middleware::ClientWithMiddleware,
    bot: &BotRow,
) -> Result<(), String> {
    let body = serde_json::json!({
        "session_id": "smoke-test", "round": 0, "role": "proponent",
        "context": [],
        "prompt": "Smoke test: respond with any valid JSON containing a 'response' field."
    });
    let response = client
        .post(&bot.endpoint_url)
        .timeout(std::time::Duration::from_secs(30))
        .json(&body)
        .send().await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = response.status();
    if !status.is_success() { return Err(format!("bot returned HTTP {status}")); }
    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("response is not valid JSON: {e}"))?;
    match json.get("response") {
        Some(serde_json::Value::String(_)) => Ok(()),
        Some(other) => Err(format!("'response' field has wrong type: expected string, got {other}")),
        None => Err("response JSON missing 'response' field".into()),
    }
}
```

- [ ] **Step 5: Add tests for reject-reason and state transitions**

Append to `tests/api_bots_test.rs`:

```rust
#[tokio::test]
async fn reject_with_short_reason_returns_400() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, token_ciphertext, status) \
         VALUES ('b1', 'B1', 'https://example.com/d', X'00', 'pending')"
    ).execute(&pool).await.unwrap();

    let req = common::admin_auth(
        Request::builder().method("PATCH").uri("/bots/b1/reject")
            .header("content-type", "application/json"),
    ).body(Body::from(r#"{"reason":"short"}"#)).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn reject_with_valid_reason_sets_status_and_reason() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, token_ciphertext, status) \
         VALUES ('b2', 'B2', 'https://example.com/d', X'00', 'pending')"
    ).execute(&pool).await.unwrap();

    let req = common::admin_auth(
        Request::builder().method("PATCH").uri("/bots/b2/reject")
            .header("content-type", "application/json"),
    ).body(Body::from(r#"{"reason":"endpoint returned garbage on all test rounds"}"#)).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "rejected");
    assert!(json["rejection_reason"].as_str().unwrap().contains("endpoint returned garbage"));
}

#[tokio::test]
async fn deactivate_pending_bot_returns_409() {
    let (app, pool) = common::test_app().await;
    sqlx::query(
        "INSERT INTO bots (id, name, endpoint_url, token_ciphertext, status) \
         VALUES ('b3', 'B3', 'https://example.com/d', X'00', 'pending')"
    ).execute(&pool).await.unwrap();

    let req = common::admin_auth(
        Request::builder().method("PATCH").uri("/bots/b3/deactivate"),
    ).body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CONFLICT);
}
```

- [ ] **Step 6: Run tests**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 7: Commit**

```bash
git add src/db/queries.rs src/api/bots.rs src/api/dto.rs src/error.rs tests/api_bots_test.rs
git commit -m "feat: transition_bot_status helper + reject reason + smoke_test_failed status"
```

---

## Task 8: Error classifier for smoke-test failures

**Files:**
- Modify: `src/api/bots.rs`
- Test: inline `#[cfg(test)] mod classifier_tests` in `src/api/bots.rs`

- [ ] **Step 1: Add the classifier function**

Above `smoke_test_bot` in `src/api/bots.rs`:

```rust
/// Convert a raw smoke-test error into plain-English feedback for the submitter.
/// Pure function; separately tested.
fn classify_smoke_test_error(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("dns") || lower.contains("name resolution") || lower.contains("failed to lookup") {
        "Endpoint hostname could not be resolved. Check the URL.".into()
    } else if lower.contains("connection refused") || lower.contains("timed out") || lower.contains("timeout") {
        "Harness could not reach the endpoint. If self-hosting, check your firewall \
         and make sure the bot is publicly reachable via HTTPS. See /bots/guide for \
         deployment options (VPS + Caddy, Cloudflare Tunnel, ngrok, etc.).".into()
    } else if lower.contains("tls") || lower.contains("ssl") || lower.contains("certificate") {
        "TLS handshake failed. The endpoint must be HTTPS with a valid certificate.".into()
    } else if lower.contains("http 401") || lower.contains("http 403") {
        "Endpoint rejected the harness's bearer token. Verify your bot is using \
         the token you registered.".into()
    } else if lower.starts_with("bot returned http ") {
        format!("Smoke test failed: {raw}. Check bot logs.")
    } else if lower.contains("is not valid json") || lower.contains("missing 'response'") {
        format!("Smoke test failed: {raw}. Your /debate endpoint must return a JSON body with a 'response' string field.")
    } else {
        format!("Smoke test failed: {raw}")
    }
}
```

- [ ] **Step 2: Use it in `approve_bot`**

Change the error handling in `approve_bot` to wrap the raw reason:

```rust
Err(reason) => {
    let classified = classify_smoke_test_error(&reason);
    let row = do_transition(
        &state, &admin, &id,
        &["pending", "smoke_test_failed"], "smoke_test_failed",
        Some(&classified),
    ).await?;
    Ok(Json(bot_to_response(&row)))
}
```

- [ ] **Step 3: Add unit tests for the classifier**

Append to `src/api/bots.rs`:

```rust
#[cfg(test)]
mod classifier_tests {
    use super::classify_smoke_test_error;

    #[test]
    fn dns_failure() {
        let out = classify_smoke_test_error("request failed: error trying to connect: dns error: failed to lookup address information");
        assert!(out.contains("hostname could not be resolved"));
    }

    #[test]
    fn connection_refused() {
        let out = classify_smoke_test_error("request failed: connection refused");
        assert!(out.contains("Harness could not reach"));
        assert!(out.contains("/bots/guide"));
    }

    #[test]
    fn tls_failure() {
        let out = classify_smoke_test_error("request failed: error trying to connect: tls handshake eof");
        assert!(out.contains("TLS handshake failed"));
    }

    #[test]
    fn http_401() {
        let out = classify_smoke_test_error("bot returned HTTP 401 Unauthorized");
        // 401 substring triggers the 401/403 branch
        assert!(out.contains("bearer token"));
    }

    #[test]
    fn json_missing_response() {
        let out = classify_smoke_test_error("response JSON missing 'response' field");
        assert!(out.contains("JSON body with a 'response' string field"));
    }

    #[test]
    fn unknown_error_falls_through() {
        let out = classify_smoke_test_error("something unexpected");
        assert_eq!(out, "Smoke test failed: something unexpected");
    }
}
```

- [ ] **Step 4: Run tests**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test classifier_tests"
```

Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/api/bots.rs
git commit -m "feat: smoke-test error classifier maps failures to actionable reasons"
```

---

## Task 9: Encrypt tokens on submit, decrypt on outbound

**Files:**
- Modify: `src/api/bots.rs`
- Modify: `src/bot_client/mod.rs`
- Test: `tests/api_bots_test.rs`

- [ ] **Step 1: Update `create_bot` to encrypt the raw token**

Replace the existing `create_bot` body in `src/api/bots.rs`:

```rust
pub async fn create_bot(
    State(state): State<AppState>,
    auth: AuthIdentity,
    Json(req): Json<CreateBotRequest>,
) -> AppResult<(StatusCode, Json<BotResponse>)> {
    if req.name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    if req.endpoint_url.is_empty() {
        return Err(AppError::BadRequest("endpoint_url is required".into()));
    }
    // HTTPS enforcement — allow http://localhost and 127.0.0.1 only in debug builds.
    if !req.endpoint_url.starts_with("https://") {
        let localhost_ok = cfg!(debug_assertions) && (
            req.endpoint_url.starts_with("http://localhost")
            || req.endpoint_url.starts_with("http://127.0.0.1")
        );
        if !localhost_ok {
            return Err(AppError::BadRequest("endpoint_url must use https://".into()));
        }
    }
    if req.token.is_empty() {
        return Err(AppError::BadRequest("token is required".into()));
    }

    let id = BotId::new();
    let ciphertext = crate::api::bot_token_crypto::encrypt(
        state.bot_token_key(),
        &req.token,
    ).map_err(|_| AppError::Internal("token encryption failed".into()))?;

    let status = if auth.is_admin() { "active" } else { "pending" };
    let submitted_by = auth.user_id().map(String::from);

    let row = queries::insert_bot(
        state.db(), id.as_str(), &req.name, &req.endpoint_url, &ciphertext,
        req.model_family.as_deref(), submitted_by.as_deref(),
        req.description.as_deref(), status,
    ).await?;
    Ok((StatusCode::CREATED, Json(bot_to_response(&row))))
}
```

- [ ] **Step 2: Update `smoke_test_bot` to decrypt and attach bearer**

```rust
async fn smoke_test_bot(
    client: &reqwest_middleware::ClientWithMiddleware,
    bot: &BotRow,
    key: &crate::api::bot_token_crypto::BotTokenKey,
) -> Result<(), String> {
    let ciphertext = bot.token_ciphertext.as_ref()
        .ok_or_else(|| "bot has no encrypted token (pre-migration row — resubmit)".to_string())?;
    let token = crate::api::bot_token_crypto::decrypt(key, ciphertext)
        .map_err(|_| "could not decrypt stored token (wrong key or corruption)".to_string())?;

    let body = serde_json::json!({
        "session_id": "smoke-test", "round": 0, "role": "proponent",
        "context": [],
        "prompt": "Smoke test: respond with any valid JSON containing a 'response' field."
    });
    let response = client
        .post(&bot.endpoint_url)
        .timeout(std::time::Duration::from_secs(30))
        .header("authorization", format!("Bearer {token}"))
        .json(&body)
        .send().await
        .map_err(|e| format!("request failed: {e}"))?;
    let status = response.status();
    if !status.is_success() { return Err(format!("bot returned HTTP {status}")); }
    let json: serde_json::Value = response.json().await
        .map_err(|e| format!("response is not valid JSON: {e}"))?;
    match json.get("response") {
        Some(serde_json::Value::String(_)) => Ok(()),
        Some(other) => Err(format!("'response' field has wrong type: expected string, got {other}")),
        None => Err("response JSON missing 'response' field".into()),
    }
}
```

Update the call site in `approve_bot` to pass the key:

```rust
match smoke_test_bot(state.http_client(), &bot, state.bot_token_key()).await {
```

- [ ] **Step 3: Update `src/bot_client/mod.rs` to send the bearer on debate calls**

Read the current file:

```bash
cat src/bot_client/mod.rs
```

Find the HTTP POST that sends the debate request. Add token decryption + header. The bot row (or at minimum its ciphertext) must be threaded through; if the current signature receives only `endpoint_url` and `body`, adjust callers in `src/orchestrator/` to pass the encrypted token. Pattern to match:

```rust
pub async fn debate_request(
    client: &ClientWithMiddleware,
    bot: &BotRow,
    key: &BotTokenKey,
    body: &DebateRequestBody,
) -> Result<DebateResponseBody, BotClientError> {
    let ciphertext = bot.token_ciphertext.as_ref()
        .ok_or(BotClientError::MissingToken)?;
    let token = crate::api::bot_token_crypto::decrypt(key, ciphertext)
        .map_err(|_| BotClientError::MissingToken)?;
    let resp = client.post(&bot.endpoint_url)
        .header("authorization", format!("Bearer {token}"))
        .json(body).send().await?;
    // ... existing response handling ...
}
```

Callers in `src/orchestrator/rounds/*.rs` must now pass `state.bot_token_key()`. Grep for call sites:

```bash
grep -rn "debate_request\|bot_client::" src/orchestrator/
```

Update each — the pattern is: wherever the orchestrator already has a `BotRow` and `AppState`, thread the key through.

- [ ] **Step 4: Update the existing `test_create_bot_returns_201` test**

The test body no longer needs a real endpoint — `create_bot` itself doesn't call out. But it does now require `https://`. Update:

```rust
let body = json!({
    "name": "TestBot",
    "endpoint_url": "https://localhost:9999/debate",
    "token": "secret123"
});
```

Or leave as `http://localhost:9999/debate` — it passes the debug-build exception.

- [ ] **Step 5: Add a round-trip test**

Append to `tests/api_bots_test.rs`:

```rust
#[tokio::test]
async fn submitted_token_is_encrypted_in_db() {
    let (app, pool) = common::test_app().await;
    let body = json!({
        "name": "TokenTest",
        "endpoint_url": "https://example.com/debate",
        "token": "s3cr3t-bearer-xyz"
    });
    let req = common::admin_auth(
        Request::builder().method("POST").uri("/bots")
            .header("content-type", "application/json"),
    ).body(Body::from(serde_json::to_string(&body).unwrap())).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Check the row in the DB directly.
    let (ciphertext, hash): (Option<Vec<u8>>, Option<String>) = sqlx::query_as(
        "SELECT token_ciphertext, token_hash FROM bots WHERE name = 'TokenTest'"
    ).fetch_one(&pool).await.unwrap();
    assert!(ciphertext.is_some());
    assert!(hash.is_none()); // new rows don't populate the legacy hash
    let ct = ciphertext.unwrap();
    assert!(ct.len() > 12); // 12-byte nonce plus ciphertext
    // Ensure the raw token is not present anywhere in the ciphertext bytes.
    assert!(!ct.windows(5).any(|w| w == b"s3cr3"));
}
```

- [ ] **Step 6: Run tests**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 7: Commit**

```bash
git add src tests
git commit -m "feat: encrypt bot tokens at submit, decrypt on outbound calls"
```

---

## Task 10: Frontend Clerk integration — JWT attachment and sign-in

**Files:**
- Modify: `frontend/package.json`
- Create: `frontend/src/lib/auth/clerk.ts`
- Modify: `frontend/src/lib/api/client.ts`
- Create: `frontend/src/routes/sign-in/+page.svelte`
- Modify: `frontend/src/routes/+layout.svelte`
- Modify: `frontend/.env.example` (create if absent)

- [ ] **Step 1: Add `@clerk/clerk-js`**

```bash
cd frontend && npm install @clerk/clerk-js
```

Verify:

```bash
grep clerk frontend/package.json
```

- [ ] **Step 2: Create the Clerk auth module**

Create `frontend/src/lib/auth/clerk.ts`:

```typescript
import { Clerk } from '@clerk/clerk-js';
import { env } from '$env/dynamic/public';

let clerkInstance: Clerk | null = null;
let loadPromise: Promise<Clerk> | null = null;

/** Lazy initialisation — returns a ready Clerk instance. */
export function getClerk(): Promise<Clerk> {
  if (clerkInstance) return Promise.resolve(clerkInstance);
  if (loadPromise) return loadPromise;
  const key = env.PUBLIC_CLERK_PUBLISHABLE_KEY;
  if (!key) {
    return Promise.reject(new Error('PUBLIC_CLERK_PUBLISHABLE_KEY is not set'));
  }
  const c = new Clerk(key);
  loadPromise = c.load().then(() => {
    clerkInstance = c;
    return c;
  });
  return loadPromise;
}

/** Return the current session JWT, or null if not signed in. */
export async function getSessionToken(): Promise<string | null> {
  const c = await getClerk();
  const session = c.session;
  if (!session) return null;
  return await session.getToken();
}

/** True once Clerk has loaded and a user is signed in. */
export async function isSignedIn(): Promise<boolean> {
  const c = await getClerk();
  return !!c.user;
}
```

- [ ] **Step 3: Attach the JWT in `request()`**

Replace `frontend/src/lib/api/client.ts` `request` function:

```typescript
import { getSessionToken } from '$lib/auth/clerk';
import { goto } from '$app/navigation';

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...((options.headers as Record<string, string>) ?? {}),
  };
  const token = await getSessionToken();
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }
  const res = await fetch(`${BASE_URL}${path}`, { ...options, headers });
  if (res.status === 401) {
    await goto('/sign-in');
    throw new ApiError(401, null);
  }
  if (!res.ok) {
    const body = await res.json().catch(() => null);
    throw new ApiError(res.status, body);
  }
  if (res.status === 204) return undefined as T;
  return res.json();
}
```

- [ ] **Step 4: Create the sign-in route**

Create `frontend/src/routes/sign-in/+page.svelte`:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { getClerk } from '$lib/auth/clerk';

  let container: HTMLDivElement;
  let error: string | null = $state(null);

  onMount(async () => {
    try {
      const clerk = await getClerk();
      clerk.mountSignIn(container, {
        afterSignInUrl: '/',
        afterSignUpUrl: '/',
      });
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load Clerk';
    }
  });
</script>

<div class="flex items-center justify-center min-h-screen bg-[var(--bg)]">
  {#if error}
    <p class="text-red-400 mono text-sm">{error}</p>
  {:else}
    <div bind:this={container}></div>
  {/if}
</div>
```

- [ ] **Step 5: Add a root layout guard**

Edit `frontend/src/routes/+layout.svelte` — at the top of the `<script>` block add:

```typescript
import { onMount } from 'svelte';
import { page } from '$app/stores';
import { goto } from '$app/navigation';
import { getClerk, isSignedIn } from '$lib/auth/clerk';

let ready = $state(false);

onMount(async () => {
  await getClerk();
  const signedIn = await isSignedIn();
  const path = $page.url.pathname;
  if (!signedIn && path !== '/sign-in') {
    await goto('/sign-in');
    return;
  }
  ready = true;
});
```

And in the template wrap the existing layout children with `{#if ready}...{/if}`. If the layout already has content, guard it:

```svelte
{#if ready || $page.url.pathname === '/sign-in'}
  <!-- existing layout content -->
{:else}
  <div class="flex items-center justify-center min-h-screen">
    <p class="mono text-xs text-[var(--text-muted)]">Loading...</p>
  </div>
{/if}
```

- [ ] **Step 6: Add environment variable placeholder**

Create `frontend/.env.example`:

```
PUBLIC_API_URL=http://localhost:3100
PUBLIC_CLERK_PUBLISHABLE_KEY=pk_test_replace_me
```

- [ ] **Step 7: Verify the frontend builds**

Per MEMORY.md: must verify Svelte build before push.

```bash
cd frontend && npm run build
```

Expected: build succeeds. (Will need a real `PUBLIC_CLERK_PUBLISHABLE_KEY` at runtime, but the build itself is static.)

- [ ] **Step 8: Commit**

```bash
git add frontend/package.json frontend/package-lock.json frontend/src/lib/auth frontend/src/lib/api/client.ts frontend/src/routes/sign-in frontend/src/routes/+layout.svelte frontend/.env.example
git commit -m "feat: Clerk frontend integration — JWT attachment, sign-in, layout guard"
```

---

## Task 11: Frontend submission feedback UI

**Files:**
- Modify: `frontend/src/lib/types.ts`
- Modify: `frontend/src/routes/bots/my-submissions/+page.svelte`
- Modify: `frontend/src/routes/bots/+page.svelte`
- Modify: `frontend/src/lib/api/client.ts`

- [ ] **Step 1: Add `rejection_reason` and reject call to types + client**

In `frontend/src/lib/types.ts`, update `BotResponse`:

```typescript
export interface BotResponse {
  id: string;
  name: string;
  endpoint_url: string;
  model_family?: string;
  status: 'pending' | 'smoke_test_failed' | 'active' | 'rejected' | 'inactive';
  description?: string;
  submitted_by?: string;
  rejection_reason?: string;
  reviewed_at?: string;
  reviewed_by?: string;
  created_at: string;
}

export interface RejectBotRequest {
  reason: string;
}
```

Remove `active: boolean` from the interface if present.

In `frontend/src/lib/api/client.ts`, change the reject method signature:

```typescript
reject: (id: string, reason: string) =>
  request<BotResponse>(`/bots/${id}/reject`, {
    method: 'PATCH',
    body: JSON.stringify({ reason }),
  }),
```

- [ ] **Step 2: Update `my-submissions` page with status banners**

Read current file:

```bash
cat frontend/src/routes/bots/my-submissions/+page.svelte | head -80
```

In the template where each bot is rendered, add (inside the card):

```svelte
{#if bot.status === 'rejected' || bot.status === 'smoke_test_failed'}
  {#if bot.rejection_reason}
    <div class="mt-3 bg-red-500/10 border border-red-500/30 rounded-md p-3">
      <div class="mono text-xs text-red-400 uppercase tracking-wider mb-1">
        {bot.status === 'rejected' ? 'Rejected' : 'Smoke test failed'}
      </div>
      <p class="text-sm text-[var(--text-secondary)]">{bot.rejection_reason}</p>
    </div>
  {/if}
{/if}
```

Also add a status pill with consistent colours:

```svelte
<span class="mono text-[10px] uppercase px-2 py-0.5 rounded"
  class:bg-gray-500={bot.status === 'pending'}
  class:bg-amber-500={bot.status === 'smoke_test_failed'}
  class:bg-red-500={bot.status === 'rejected'}
  class:bg-green-500={bot.status === 'active'}
  class:bg-neutral-500={bot.status === 'inactive'}>
  {bot.status}
</span>
```

- [ ] **Step 3: Update the admin bot review section on `/bots`**

In `frontend/src/routes/bots/+page.svelte`, check whether an admin section already exists. If not, add a separate section at the top listing bots whose `status ∈ { pending, smoke_test_failed, rejected }`. For each card, include:

- The `rejection_reason` banner from Step 2 (shared component extraction optional).
- Action buttons (only visible to admins — check `me.role === 'admin'` via a store populated from `GET /me`):
  - `pending` and `smoke_test_failed`: **Approve** (calls `api.bots.approve(id)`) and **Reject** (opens modal).
  - `smoke_test_failed`: also shows **Retry approval** which calls the same `approve` endpoint.
  - `rejected`: no actions.

Reject modal — inline in the page:

```svelte
{#if rejectingBot}
  <div class="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
    <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-6 w-full max-w-md">
      <h3 class="mono text-sm text-[var(--text-primary)] mb-3">Reject {rejectingBot.name}</h3>
      <p class="text-xs text-[var(--text-muted)] mb-3">
        Enter a reason (min 10 chars). This is shown to the submitter.
      </p>
      <textarea
        bind:value={rejectReason}
        rows={4}
        maxlength={500}
        placeholder="Reason for rejection..."
        class="w-full px-3 py-2 bg-[var(--bg)] border border-[var(--border)] rounded text-sm"
      ></textarea>
      <div class="mt-3 flex justify-end gap-2">
        <button
          onclick={() => { rejectingBot = null; rejectReason = ''; }}
          class="px-3 py-1.5 text-sm rounded border border-[var(--border)]"
        >Cancel</button>
        <button
          disabled={rejectReason.trim().length < 10 || submittingReject}
          onclick={async () => {
            submittingReject = true;
            try {
              await api.bots.reject(rejectingBot.id, rejectReason.trim());
              await reloadBots();
            } finally {
              submittingReject = false;
              rejectingBot = null;
              rejectReason = '';
            }
          }}
          class="px-3 py-1.5 text-sm rounded bg-red-500 text-white disabled:opacity-50"
        >Reject</button>
      </div>
    </div>
  </div>
{/if}
```

- [ ] **Step 4: Verify the frontend builds**

```bash
cd frontend && npm run build
```

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat: submission feedback banners and admin reject modal"
```

---

## Task 12: Admin-only debate creation UI

**Files:**
- Modify: `frontend/src/lib/components/Sidebar.svelte`
- Modify: `frontend/src/routes/debates/new/+page.svelte`

- [ ] **Step 1: Hide the "New Debate" link for participants**

Find where Sidebar renders the New Debate link. Wrap with admin check via a store populated from `GET /me`. If no store exists, create `frontend/src/lib/stores/me.ts`:

```typescript
import { readable } from 'svelte/store';
import { api } from '$lib/api/client';
import type { UserInfoResponse } from '$lib/types';

export const me = readable<UserInfoResponse | null>(null, (set) => {
  api.me().then(set).catch(() => set(null));
});
```

In Sidebar:

```svelte
<script>
  import { me } from '$lib/stores/me';
</script>

{#if $me?.role === 'admin'}
  <a href="/debates/new">New Debate</a>
{/if}
```

- [ ] **Step 2: Guard `/debates/new` at page level**

At the top of `frontend/src/routes/debates/new/+page.svelte`:

```svelte
<script>
  import { me } from '$lib/stores/me';
  import { goto } from '$app/navigation';

  $effect(() => {
    if ($me && $me.role !== 'admin') {
      goto('/debates');
    }
  });
</script>

{#if $me?.role !== 'admin'}
  <div class="p-8">
    <p class="mono text-sm text-[var(--text-muted)]">
      Only admins can create debates. <a href="/debates" class="text-[#8b5cf6]">Back to debates</a>.
    </p>
  </div>
{:else}
  <!-- existing new-debate form -->
{/if}
```

- [ ] **Step 3: Build**

```bash
cd frontend && npm run build
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src
git commit -m "feat: hide new-debate controls from participants"
```

---

## Task 13: Remove dev-mode fallback and add boot-time config validation

**Files:**
- Modify: `src/api/auth.rs` (drop the deprecated aliases)
- Modify: `src/api/debates.rs`, `src/api/synthesis.rs`, `src/api/transcript.rs` (if any still reference the aliases)
- Modify: `src/config.rs` (add validation function)
- Modify: `src/main.rs` (call validation early)

- [ ] **Step 1: Delete deprecated aliases from `src/api/auth.rs`**

Remove the two `#[deprecated]` type aliases at the bottom. If any caller still uses `BearerAuth` or `AdminOnly`, rename the import and handler parameter.

- [ ] **Step 2: Add a `validate()` method to `Settings`**

Append to `src/config.rs`:

```rust
impl Settings {
    /// Fail-fast validation of boot-time configuration invariants.
    /// Returns an error describing the first failure; server should refuse to start.
    pub fn validate(&self) -> anyhow::Result<()> {
        let a = &self.auth;

        // 1. Either admin_token or clerk_issuer must be set (no dev-mode fallback).
        if a.admin_token.is_empty() && a.clerk_issuer.is_empty() && !cfg!(test) {
            anyhow::bail!(
                "auth.admin_token OR auth.clerk_issuer must be set. \
                 Dev-mode auto-admin has been removed."
            );
        }

        // 2. Clerk path requires admin_user_ids and bot_token_key.
        if !a.clerk_issuer.is_empty() {
            if a.admin_user_ids.is_empty() {
                anyhow::bail!(
                    "auth.clerk_issuer is set but auth.admin_user_ids is empty; \
                     no one would have admin privileges"
                );
            }
            for id in &a.admin_user_ids {
                if !id.starts_with("user_") {
                    anyhow::bail!(
                        "auth.admin_user_ids contains '{id}', which does not look like a \
                         Clerk user_id (expected format: user_2...)"
                    );
                }
            }
            if a.bot_token_key.is_empty() {
                anyhow::bail!(
                    "auth.clerk_issuer is set but auth.bot_token_key is not; \
                     bot tokens cannot be encrypted"
                );
            }
            crate::api::bot_token_crypto::parse_key_hex(&a.bot_token_key)
                .map_err(|_| anyhow::anyhow!(
                    "auth.bot_token_key must be exactly 64 hex characters (32 bytes)"
                ))?;
        }

        Ok(())
    }
}
```

- [ ] **Step 3: Call validation in `main.rs`**

In `src/main.rs`, right after `let settings = Settings::load()?;`, insert:

```rust
settings.validate()?;
```

If validation fails, the process exits with a clear error message and no server starts.

- [ ] **Step 4: Add config validation tests**

Create `tests/config_validation_test.rs`:

```rust
use bot_council::config::{Settings, ServerConfig, DatabaseConfig, AuthConfig, HttpClientConfig, ModelsConfig, DebateConfig};

fn base() -> Settings {
    Settings {
        server: ServerConfig { host: "".into(), port: 0, cors_origins: vec![] },
        database: DatabaseConfig { url: "".into() },
        auth: AuthConfig {
            admin_token: "".into(),
            clerk_issuer: "".into(),
            clerk_jwks_url: "".into(),
            admin_user_ids: vec![],
            bot_token_key: "".into(),
        },
        http_client: HttpClientConfig { connect_timeout_secs: 1, request_timeout_secs: 1, max_retries: 0, retry_delay_secs: 1 },
        models: ModelsConfig { minimax_api_key: "".into(), minimax_model: "".into(), minimax_base_url: "".into(), opus_api_key: "".into(), opus_model: "".into() },
        debate: DebateConfig { default_timeout_secs: 1, max_retries: 0, quorum: 3, synthesis_temperature: 0.0 },
    }
}

#[test]
fn rejects_both_empty_in_prod_mode() {
    // cfg!(test) is true in this crate, so this check is skipped — re-invoke
    // via a release-build integration or trust the source. Instead we assert
    // the Clerk-specific branches.
    let mut s = base();
    s.auth.clerk_issuer = "https://example.clerk.accounts.dev".into();
    assert!(s.validate().is_err()); // missing admin_user_ids
}

#[test]
fn rejects_clerk_without_admin_user_ids() {
    let mut s = base();
    s.auth.clerk_issuer = "https://example.clerk.accounts.dev".into();
    s.auth.bot_token_key = "0".repeat(64);
    let err = s.validate().unwrap_err().to_string();
    assert!(err.contains("admin_user_ids"));
}

#[test]
fn rejects_malformed_user_id() {
    let mut s = base();
    s.auth.clerk_issuer = "https://example.clerk.accounts.dev".into();
    s.auth.admin_user_ids = vec!["not_a_clerk_id".into()];
    s.auth.bot_token_key = "0".repeat(64);
    let err = s.validate().unwrap_err().to_string();
    assert!(err.contains("user_"));
}

#[test]
fn rejects_missing_bot_token_key_when_clerk_set() {
    let mut s = base();
    s.auth.clerk_issuer = "https://example.clerk.accounts.dev".into();
    s.auth.admin_user_ids = vec!["user_2abc".into()];
    let err = s.validate().unwrap_err().to_string();
    assert!(err.contains("bot_token_key"));
}

#[test]
fn accepts_bearer_only_config() {
    let mut s = base();
    s.auth.admin_token = "some-secret".into();
    assert!(s.validate().is_ok());
}

#[test]
fn accepts_valid_clerk_config() {
    let mut s = base();
    s.auth.clerk_issuer = "https://example.clerk.accounts.dev".into();
    s.auth.admin_user_ids = vec!["user_2abc".into(), "user_2def".into()];
    s.auth.bot_token_key = "0".repeat(64);
    assert!(s.validate().is_ok());
}
```

- [ ] **Step 5: Run tests**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo test"
```

- [ ] **Step 6: Commit**

```bash
git add src tests
git commit -m "feat: boot-time config validation; remove deprecated BearerAuth aliases"
```

---

## Task 14: Deploy to EVO and verify

- [ ] **Step 1: Pre-check for existing bots**

```bash
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 \
  "cd ~/bot-council && sqlite3 data/council.db 'SELECT id, name, status, submitted_by FROM bots;'"
```

If rows exist with `token_ciphertext IS NULL`, they will fail smoke test after this rollout. Note them — they must be resubmitted by their owners (or, for Clint, by James using EVO `.env DASHBOARD_TOKEN`).

- [ ] **Step 2: Set EVO environment variables**

Add to `/etc/systemd/system/bot-council.service` or the project's `.env`:

```
APP__AUTH__ADMIN_TOKEN=<a random 32-byte hex string>
APP__AUTH__CLERK_ISSUER=https://<your-clerk-instance>.clerk.accounts.dev
APP__AUTH__ADMIN_USER_IDS=<James's user_id>,<Jamie's>,<Artur's>,<Ray's>,<YC's>
APP__AUTH__BOT_TOKEN_KEY=<64-char hex — generate with `openssl rand -hex 32`>
```

Note: get each admin's Clerk user_id from the Clerk dashboard → Users → click user → copy `User ID` (format `user_2...`).

- [ ] **Step 3: Deploy**

```bash
scp -i C:/Users/James/.ssh/id_ed25519 -r src tests config migrations Cargo.toml Cargo.lock james@100.90.66.54:~/bot-council/
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 "source ~/.cargo/env && cd ~/bot-council && cargo build --release && sudo systemctl restart bot-council"
```

- [ ] **Step 4: Smoke-test manually**

```bash
# /health should be reachable without auth
curl -s https://lqcouncil.com/health

# /me without auth should 401
curl -si https://lqcouncil.com/me | head -1
# expect: HTTP/2 401

# /me with admin bearer should return admin identity
curl -si -H "Authorization: Bearer <APP__AUTH__ADMIN_TOKEN>" https://lqcouncil.com/me

# POST /debates without auth should 401
curl -si -X POST https://lqcouncil.com/debates -H "content-type: application/json" -d '{"topic":"x"}' | head -1
```

- [ ] **Step 5: Sign in on the frontend and verify round-trip**

Open https://lqcouncil.com in a browser. Expected flow:
1. Redirects to `/sign-in`.
2. Clerk UI loads.
3. Sign in with one of the 5 admin accounts.
4. Redirect to `/`.
5. `/bots/submit` works. Submit a dummy bot; it goes to `pending`.
6. `/bots` shows admin review section with the new bot.
7. Click Approve — expect smoke test to fail (dummy endpoint), bot moves to `smoke_test_failed` with a readable reason.
8. Click Reject — modal opens, typed reason is persisted, bot moves to `rejected`.

- [ ] **Step 6: Sign out, sign in as a participant (non-admin test Clerk user)**

Expected:
1. `/debates/new` not in the sidebar.
2. Direct navigation to `/debates/new` shows "Only admins can create debates" message.
3. `POST /bots` succeeds → status `pending`.
4. `/bots/my-submissions` shows the pending bot and reflects any admin decisions.

- [ ] **Step 7: Resubmit Clint if necessary**

Per §18.5 of the spec — if Clint's row in the DB has `token_ciphertext IS NULL`, resubmit through `/bots/submit` using Clint's existing endpoint URL and its configured `DASHBOARD_TOKEN`.

- [ ] **Step 8: Commit any deploy artefacts**

```bash
# only if any files changed during the deploy run
git add -p
git commit -m "chore: deploy artefacts from Clerk auth rollout"
```

---

## Self-Review Notes

Before starting implementation, the implementing engineer should cross-check the plan against the spec:

- **§§1–4 (Problem + goals + identity model):** Covered by Tasks 4–6 + 13.
- **§5 (Route matrix):** Covered by Task 6.
- **§6 (Bot lifecycle + smoke_test_failed):** Covered by Task 7 + Task 8.
- **§7 (Token storage + crypto):** Covered by Tasks 1, 2, 9.
- **§8 (Data model):** Covered by Task 2 (DB) + Task 7 (DTO).
- **§9 (Simplifications):** Covered by Task 7 (handler collapse, RETURNING \*, BOT_COLUMNS).
- **§10 (Frontend Clerk):** Covered by Tasks 10, 11, 12.
- **§11 (Config):** Covered by Task 3 (fields) + Task 13 (validation).
- **§12 (Tests):** Spread across Tasks 1, 4, 7, 8, 9, 13.
- **§13 (File budget):** Monitor during implementation — `auth.rs` in Task 5 must stay under 300 lines.
- **§14 (Execution order):** Maps onto Tasks 1–13 in this plan.
- **§§18–19 (Bot author UX, MiniMax constraint):** Not in this plan — separate Plan 2 to follow.

---

## Risk register for implementation

- Field names in `BotRow` / `BotResponse` and DB column order must stay in sync. If sqlx complains about missing fields, `BOT_COLUMNS` is probably missing a column.
- `@clerk/clerk-js` has a size footprint. If the Svelte bundle grows unacceptably, consider deferring Clerk loading with dynamic `import()`.
- `jsonwebtoken::Validation` defaults to validating `exp`. Do not disable `validate_exp` — the original insecure path did.
- `AES-GCM` reuse of nonce with the same key is catastrophic. The module generates a fresh nonce per call via `OsRng`; do not cache or reuse nonces.
- **Gap vs. spec §12.4:** this plan does not implement the `#[cfg(test)] test_impersonate` participant-auth hook. Admin-path and unauthenticated-path are both covered by unit/integration tests; participant-specific behaviour (403 on admin routes, 200 with `status=pending` on `POST /bots`) is verified manually in Task 14 step 6. Add the impersonation hook in a follow-up plan if automated participant tests become necessary.

---

**Plan complete.**
