# Clerk Auth, RBAC, and Bot Submission Cleanup

**Date:** 2026-04-16
**Status:** Draft — awaiting user approval before implementation plan
**Scope:** Replace insecure JWT decode with proper Clerk JWKS verification; introduce
role-based access control with a 5-person admin allowlist; fix the bot submission
pipeline so rejected/failed bots return actionable feedback to their submitter; apply
previously identified simplifications (handler collapse, `RETURNING *`, dead code removal)
while the auth surface is already being touched.

---

## 1. Problem Statement

The current bot submission pipeline is non-functional in realistic configurations:

1. **Frontend never attaches a Clerk JWT.** In dev-mode (both `admin_token` and
   `clerk_issuer` empty) every request is treated as admin, silently auto-approving
   submissions and bypassing the review workflow. In prod-mode (Clerk configured) the
   frontend cannot authenticate at all.
2. **Clerk JWT signature verification is disabled** (`insecure_disable_signature_validation`).
   Any attacker with the issuer string can forge admin claims.
3. **The smoke test sends no bearer token to bot endpoints.** Any bot that authenticates
   its `/debate` endpoint (which production bots should) will reject the smoke test and
   be un-approvable.
4. **Rejection and smoke-test failure provide no actionable feedback.** Failures surface
   as transient HTTP 400 JSON blobs to the admin clicking the button, then vanish.
   Submitters never learn why their bot was rejected.
5. **Vestigial complexity**: four near-identical PATCH handlers; duplicated
   `get_bot → check → update → get_bot` pattern; `active` column redundant with
   `status`; `BearerAuth` alias with no remaining justification; mixed timestamp
   formats between `created_at` (SQLite) and `reviewed_at` (RFC3339).

## 2. Goals

- Cryptographically correct Clerk JWT verification (RS256 via JWKS).
- Clean split: admin vs. participant, enforced at route level.
- Hard-coded allowlist of 5 admin Clerk user_ids (James, Jamie, Artur, Ray, YC).
- Participants can submit bots, view their own submissions, view active bots, read debate
  content. Cannot create debates or administer bots.
- Smoke-test failures and admin rejections surface reasons to the submitter.
- Bot bearer tokens are recoverable for outbound calls (encrypted at rest, not hashed).
- Eliminate dead code and duplicated patterns identified in the pre-design review.

## 3. Non-Goals

- Organization/team model beyond the flat admin/participant split.
- Audit log of all bot state transitions (future work — only the most recent
  `rejection_reason` is kept).
- Per-debate RBAC (e.g. "only admin can stream debate X"). Debate access is binary:
  signed-in or not.
- Migrating existing bot tokens. The migration is additive and the existing `token_hash`
  column is retained until a follow-up release drops it.

## 4. Identity Model

### 4.1 AuthIdentity

```rust
pub enum AuthIdentity {
    Admin { user_id: Option<String>, source: AuthSource },
    Participant { user_id: String },
}

pub enum AuthSource {
    BearerToken,   // matched config.auth.admin_token
    ClerkJwt,      // valid Clerk JWT with sub ∈ admin_user_ids
}
```

`user_id` on the Admin variant is `None` when authenticated via `admin_token`
(the bearer is an impersonal CLI/emergency credential).

### 4.2 Extractors

Two axum extractors replace the current `BearerAuth` alias:

- `RequireAuth(AuthIdentity)` — succeeds for any Admin or Participant, 401 otherwise.
- `RequireAdmin(AuthIdentity)` — succeeds only for `AuthIdentity::Admin`, 403 otherwise.

The existing `BearerAuth` alias and `AdminOnly` struct are deleted. Four call sites
(`debates.rs`, `synthesis.rs`, `transcript.rs`) are updated to the new names.

### 4.3 JWT verification

- JWKS fetched at startup from `{clerk_issuer}/.well-known/jwks.json` (or
  `clerk_jwks_url` if explicitly set).
- Key set cached in `AppState` behind `Arc<ArcSwap<JwkSet>>`.
- Background Tokio task refreshes the set every 10 minutes. On fetch failure the
  cached set is retained; only a startup failure blocks boot.
- `jsonwebtoken::decode` with `Algorithm::RS256`, validating signature, issuer,
  expiry, and default clock skew. `insecure_disable_signature_validation` is deleted.
- On JWKS rotation (`kid` not in cache), the verifier triggers a synchronous refresh
  before rejecting the token.

### 4.4 Admin decision

```rust
fn is_admin(claims: &ClerkClaims, cfg: &AuthConfig) -> bool {
    cfg.admin_user_ids.iter().any(|id| id == &claims.sub)
}
```

No metadata inspection, no email dependency, no organization lookup. The allowlist is
the single source of truth.

### 4.5 Dev-mode removal

The current "both empty → auto-admin" fallback is deleted. Boot-time config
validation:

- If `clerk_issuer` is set, `admin_user_ids` must be non-empty.
- If `clerk_issuer` is set, `bot_token_key` must be set and decode to 32 bytes.
- If neither `clerk_issuer` nor `admin_token` is set, server refuses to start
  (unless `cfg!(test)`).

## 5. Route Matrix

| Method | Path | Extractor | Notes |
|---|---|---|---|
| GET | /health | none | Public |
| GET | /me | RequireAuth | Returns role + user_id |
| GET | /bots | RequireAuth | Admin: all bots. Participant: active only |
| POST | /bots | RequireAuth | Admin → status=active. Participant → status=pending |
| GET | /bots/my-submissions | RequireAuth | Rejects bearer-only auth (400); requires Clerk user_id |
| PATCH | /bots/{id}/approve | RequireAdmin | Runs smoke test. See §6 |
| PATCH | /bots/{id}/reject | RequireAdmin | Body `{"reason": string}` required, min 10 chars |
| PATCH | /bots/{id}/deactivate | RequireAdmin | from=active → inactive |
| PATCH | /bots/{id}/reactivate | RequireAdmin | from=inactive → active |
| GET | /debates, /debates/{id}, /debates/{id}/transcript, /debates/{id}/synthesis, /debates/{id}/stream | RequireAuth | |
| POST | /debates | RequireAdmin | **New restriction** — participants cannot create debates |

## 6. Bot Lifecycle

### 6.1 Status values

```
pending              ← initial state for participant submissions
smoke_test_failed    ← approve clicked but bot endpoint failed smoke test
active               ← approved and smoke test passed
rejected             ← admin explicitly rejected with reason
inactive             ← admin deactivated a previously active bot
```

### 6.2 Transitions

| From | To | Trigger | Side effect |
|---|---|---|---|
| (none) | pending | POST /bots as participant | submitted_by = user_id |
| (none) | active | POST /bots as admin | — |
| pending \| smoke_test_failed | active | PATCH /approve (smoke test passes) | Clears `rejection_reason` |
| pending \| smoke_test_failed | smoke_test_failed | PATCH /approve (smoke test fails) | Sets `rejection_reason` to failure detail. Returns 200, not 400 |
| pending \| smoke_test_failed | rejected | PATCH /reject with body | Sets `rejection_reason` to admin-supplied text |
| active | inactive | PATCH /deactivate | — |
| inactive | active | PATCH /reactivate | Clears `rejection_reason` |

All other transitions return 409 Conflict with a message naming the current and expected
states.

### 6.3 Feedback loop for submitters

- `BotResponse.rejection_reason: Option<String>` surfaces the reason set by reject or
  smoke-test failure.
- Frontend `/bots/my-submissions` renders `rejection_reason` as a red banner under bots
  in `rejected` or `smoke_test_failed` status.
- Admin bot review UI shows the same reason on the bot card and offers a "Retry
  approval" button for `smoke_test_failed` bots.
- When the admin retries approval and the smoke test passes, the bot moves to `active`
  and the reason is cleared.

### 6.4 Smoke test mechanics

- `smoke_test_bot` decrypts the stored bot token and sends `Authorization: Bearer <token>`
  on the request. Previously the call was unauthenticated.
- Payload remains a minimal JSON body identifying the request as a smoke test
  (`session_id: "smoke-test"`, `round: 0`, `role: "proponent"`, etc.). A follow-up may
  share the DTO with actual debate calls; out of scope here.
- Failure reasons returned verbatim to the caller in the response body AND persisted as
  `rejection_reason` on the bot row.

## 7. Bot Token Storage

### 7.1 Migration

Single new migration `20260416000001_bot_submission_cleanup.sql`:

```sql
ALTER TABLE bots ADD COLUMN token_ciphertext BLOB;
ALTER TABLE bots ADD COLUMN rejection_reason TEXT;
-- token_hash and active columns are retained during rollout.
-- A follow-up migration drops them after one release.

CREATE INDEX idx_bots_status_reviewable
    ON bots(status)
    WHERE status IN ('pending', 'smoke_test_failed');
```

New rows populate `token_ciphertext`. Old rows (if any exist — see §16 risk
pre-check) have `token_ciphertext IS NULL` and their smoke test will fail with a
clear error asking the submitter to re-submit the bot.

### 7.2 Crypto

New module `src/api/bot_token_crypto.rs` (~80 lines):

```rust
pub fn encrypt(key: &[u8; 32], plaintext: &str) -> Result<Vec<u8>>;
pub fn decrypt(key: &[u8; 32], ciphertext: &[u8]) -> Result<String>;
```

- AES-256-GCM via the `aes-gcm` crate.
- Random 12-byte nonce generated per encryption, prepended to output ciphertext.
- Output layout: `[12-byte nonce][ciphertext || 16-byte auth tag]`.
- Key loaded at boot from `APP__AUTH__BOT_TOKEN_KEY` (64-char hex string).

### 7.3 Outbound calls

Both `smoke_test_bot` and `src/bot_client` decrypt the stored ciphertext and attach
`Authorization: Bearer <token>`. Decryption failure on an outbound call fails the call
with a structured error (`CryptoError::Decrypt`) mapped to a 500 with a generic message.
The detailed failure is logged at tracing ERROR level.

## 8. Data Model Changes

### 8.1 `BotRow` (src/db/models.rs)

```rust
pub struct BotRow {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub token_hash: Option<String>,        // kept for backward-read compat
    pub token_ciphertext: Option<Vec<u8>>, // new, required for new rows
    pub model_family: Option<String>,
    pub status: String,
    pub submitted_by: Option<String>,
    pub description: Option<String>,
    pub rejection_reason: Option<String>,  // new
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
    pub created_at: String,
}
```

The `active: bool` column is **removed** from `BotRow` (it's derived from `status`).
The DB column stays for now; a follow-up migration drops it after all handlers are
confirmed ignoring it. Frontend-visible `BotResponse.active` is likewise removed;
the frontend computes `status === 'active'` if needed.

### 8.2 `BotResponse` (src/api/dto.rs)

```rust
pub struct BotResponse {
    pub id: String,
    pub name: String,
    pub endpoint_url: String,
    pub model_family: Option<String>,
    pub status: String,
    pub description: Option<String>,
    pub submitted_by: Option<String>,
    pub rejection_reason: Option<String>,  // new
    pub reviewed_at: Option<String>,
    pub reviewed_by: Option<String>,
    pub created_at: String,
}
```

`token_hash` and `token_ciphertext` are never serialized (no public or API surface).

### 8.3 New reject request DTO

```rust
pub struct RejectBotRequest {
    pub reason: String,
}
```

Validated: `reason.trim().len() >= 10`, `<= 500`.

## 9. Simplifications Applied

### 9.1 Handler collapse

`approve_bot`, `reject_bot`, `deactivate_bot`, `reactivate_bot` currently repeat
`fetch → verify status → update → re-fetch`. Replaced by:

```rust
async fn transition_bot_status(
    pool: &SqlitePool,
    id: &str,
    expected_from: &[&str],
    new_status: &str,
    reviewed_by: Option<&str>,
    rejection_reason: Option<&str>,
) -> Result<BotRow, AppError>
```

Using `UPDATE ... WHERE status IN (?, ?, ...) RETURNING *`. If the row is affected,
the updated `BotRow` is returned. If zero rows affected, the function fetches the
current row and returns `AppError::Conflict` with the actual vs. expected states.

Each PATCH handler becomes 3–5 lines. Approve inserts a smoke test between the status
check and the state change.

### 9.2 `RETURNING *`

`update_bot_status` now returns `BotRow`, eliminating the redundant second `get_bot`
in every admin PATCH handler.

### 9.3 Column-list constant

```rust
const BOT_COLUMNS: &str = "id, name, endpoint_url, token_hash, token_ciphertext, \
    model_family, status, submitted_by, description, rejection_reason, \
    reviewed_at, reviewed_by, created_at";
```

Replaces six copies of the same list across `queries.rs`.

### 9.4 Timestamp unification

`reviewed_at` switched from `chrono::Utc::now().to_rfc3339()` to SQLite
`datetime('now')`, matching `created_at`. Lexical ordering across rows becomes
consistent again.

### 9.5 Dead code removal

- `BearerAuth` type alias deleted.
- `AdminOnly` extractor replaced by `RequireAdmin`.
- Vestigial `active` field removed from `BotResponse` and `BotRow`. DB column dropped
  in follow-up migration only after one release.

## 10. Frontend Changes

### 10.1 Clerk integration

- Add `@clerk/clerk-js` to `frontend/package.json`.
- New `frontend/src/lib/auth/clerk.ts`:
  - Initializes Clerk with `PUBLIC_CLERK_PUBLISHABLE_KEY` at app load.
  - Exposes a `getToken()` helper returning the current session JWT, or `null`.
- `frontend/src/lib/api/client.ts`:
  - `request()` awaits `getToken()` and attaches `Authorization: Bearer <jwt>` if
    present.
  - On 401 response, redirects to `/sign-in`.
- New `/sign-in` route mounts Clerk's sign-in UI.
- Root `+layout.svelte` guards all routes except `/sign-in` by checking
  `clerk.user`; redirects to `/sign-in` if absent.

### 10.2 Submission feedback UI

- `frontend/src/routes/bots/my-submissions/+page.svelte`:
  - For bots in status `rejected` or `smoke_test_failed`, render `rejection_reason`
    in a red banner below the bot card.
  - Status pill colors: `pending` (gray), `smoke_test_failed` (amber), `rejected`
    (red), `active` (green), `inactive` (neutral).

- Admin bot review UI on `/bots` (when the signed-in user is admin):
  - Pending, smoke_test_failed, and rejected bots appear in a dedicated section above
    the active-bots list.
  - Each card shows `rejection_reason` prominently for `smoke_test_failed` and
    `rejected` bots.
  - Approve, Reject, and "Retry approval" (smoke_test_failed only) actions inline on
    each card.
  - Reject opens a modal dialog that captures a reason (min 10 chars) before calling
    `PATCH /reject`.

### 10.3 Participant UX for debate creation

- `/debates/new` route is admin-only. Participants attempting it see a
  "Only admins can create debates" message with a link back to the debate list.
- The "New Debate" button in the sidebar is hidden for participants.

## 11. Configuration

### 11.1 `config/default.toml`

```toml
[auth]
admin_token = ""
clerk_issuer = ""
clerk_jwks_url = ""          # auto-derived from issuer if empty
admin_user_ids = []          # array of Clerk user_2... strings
bot_token_key = ""           # 64 hex chars = 32 bytes AES-256 key
```

### 11.2 Env overrides

- `APP__AUTH__CLERK_ISSUER=https://...`
- `APP__AUTH__ADMIN_USER_IDS=user_2abc,user_2def,user_2ghi,user_2jkl,user_2mno` (comma split)
- `APP__AUTH__BOT_TOKEN_KEY=<hex>`
- `APP__AUTH__ADMIN_TOKEN=<random-secret>`

### 11.3 Boot-time validation

In `src/config.rs::load()`:

1. If `clerk_issuer != ""`:
   - `admin_user_ids` must be non-empty.
   - `bot_token_key` must parse as 32 bytes of hex.
2. If `clerk_issuer == "" && admin_token == ""`:
   - Refuse to start. Print a pointer to the setup docs.
3. If `bot_token_key` is set, verify it decodes to 32 bytes.

All failures are fatal and logged at ERROR level.

## 12. Testing

### 12.1 Crypto unit tests

- `decrypt(encrypt(s)) == s` for various lengths.
- Tampered ciphertext fails decryption with auth-tag error.
- Wrong key fails decryption.

### 12.2 Auth extractor tests

- No `Authorization` header → 401.
- Invalid JWT signature → 401.
- JWT with wrong issuer → 401.
- JWT expired → 401.
- JWT with `sub` in allowlist → Admin identity.
- JWT with `sub` not in allowlist → Participant identity.
- Static admin_token match → Admin identity (source=BearerToken).

### 12.3 Route-level tests

- `POST /bots` without auth → 401.
- `POST /bots` as Participant → 201, status=pending, submitted_by=user_id.
- `POST /bots` as Admin (bearer) → 201, status=active.
- `POST /debates` as Participant → 403.
- `POST /debates` as Admin → 201.
- `PATCH /bots/{id}/approve` without admin → 403.
- `PATCH /bots/{id}/approve` with failing smoke test → 200, status=smoke_test_failed,
  rejection_reason populated.
- `PATCH /bots/{id}/approve` on smoke_test_failed bot with now-healthy endpoint → 200,
  status=active, rejection_reason cleared.
- `PATCH /bots/{id}/reject` without body → 400.
- `PATCH /bots/{id}/reject` with reason < 10 chars → 400.
- `PATCH /bots/{id}/reject` with valid reason → 200, status=rejected, reason stored.
- `GET /bots/my-submissions` with bearer-only auth → 400.
- `GET /bots` as Participant → only `active` bots.
- `transition_bot_status` rejects wrong-state transitions with a Conflict error naming
  actual vs. expected state.

### 12.4 Test participant impersonation

A test-only config flag:

```rust
#[cfg(test)]
pub test_impersonate: Option<TestImpersonate>,
```

```rust
pub enum TestImpersonate {
    Admin,
    Participant { user_id: String },
}
```

When set and the request carries `Authorization: Bearer test-impersonate`, the extractor
short-circuits to the specified identity. The field is gated by `#[cfg(test)]` so it
cannot be enabled in production builds.

## 13. File Budget

- `src/api/auth.rs` — ~220 lines (JWKS cache + extractors + claims decoding).
- `src/api/bots.rs` — ~170 lines (down from 221 after handler collapse).
- `src/api/bot_token_crypto.rs` — ~80 lines (new).
- `src/api/jwks_cache.rs` — ~110 lines (new — JwkSet fetch/cache/refresh).
- `src/db/queries.rs` — ~200 lines (net neutral).

All under the 300-line ceiling.

## 14. Execution Order

Five commits landed in sequence within a single work session:

1. **Crypto + migration.** Add `aes-gcm` dependency, `bot_token_crypto.rs`,
   migration for `token_ciphertext` and `rejection_reason`. New tests. No handler
   changes yet.
2. **JWKS cache + new extractors.** `jwks_cache.rs`, `RequireAuth`, `RequireAdmin`.
   Full unit-test coverage. Old `AuthIdentity` enum refactored. No routes changed yet;
   `BearerAuth` alias still there so existing code compiles.
3. **Wire extractors to routes + handler collapse.** `transition_bot_status`,
   `RETURNING *`, delete `BearerAuth`/`AdminOnly`, rename the four PATCH handlers.
   `POST /debates` becomes admin-only. Submission feedback (`rejection_reason`)
   populated end-to-end.
4. **Frontend Clerk integration.** `@clerk/clerk-js`, `client.ts` JWT attachment,
   sign-in page, route guards, submission feedback UI, admin-only debate creation.
5. **Delete dev-mode fallback + boot-time config validation.** Removes the auto-admin
   trap. Last change because it's a breaking config change — requires
   `admin_token` or Clerk to be set.

Each commit is independently deployable and independently testable on the EVO.

## 15. Open Questions

None. All design decisions recorded above.

## 16. Risks and Mitigations

| Risk | Mitigation |
|---|---|
| JWKS fetch fails at startup | Log ERROR, allow server to start in bearer-only mode. Refresh loop keeps trying. Surfaced in `/health` response body. |
| Admin user_id typo in config | Boot validation confirms IDs look like `user_...`. A user with a matching-but-wrong ID would just fail the allowlist check and get Participant role — safe failure mode. |
| Bot token key rotation | Out of scope. Document as a follow-up. Rotating the key requires re-submitting all bots. |
| Existing deployed bots in DB lose auth on next debate | Pre-implementation check: query `SELECT COUNT(*) FROM bots WHERE status='active'` on the EVO DB. If non-zero, either re-submit those bots (short list) or add a one-off script to prompt the submitter to rotate the stored token before the smoke-test-with-bearer code path ships. The plan assumes zero rows; if the check fails, the implementation plan must add the script as an extra step. |
| Clerk outage blocks all frontend access | Admin bearer token path remains available for curl-based operations. |
| Test impersonation flag leaked to prod | `#[cfg(test)]` gate makes the field impossible to set in a release build. Compile-time enforcement. |

## 17. Rollback Plan

Each of the five commits can be reverted independently. The migration (§14 commit 1)
is additive — no data loss if reverted mid-rollout. The only irreversible step is
dropping `token_hash` and `active` columns, which is deferred to a follow-up release
explicitly so this plan can be rolled back cleanly.
