# Lenient Ingest + Honest Errors (Bot Lifecycle Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The reliability floor from `docs/superpowers/specs/2026-07-02-bot-lifecycle-design.md` Parts 2–3: a normalisation ladder that accepts anything prose-shaped on the existing push transport, truncation instead of rejection, an honest `auth` error kind with an owner-facing guidance table, lenient smoke validation, `ingest_kind` persistence, and sentinel `ING-001`.

**Architecture:** One new sans-io module (`bot_client/ingest.rs`) owns the ladder; both dispatch paths and the smoke validator consume it. `ingest_kind` rides on `DebateRoundResponse` (serde-skipped) and threads into `insert_response_full`. Guidance strings live in their own module so wizard/monitoring/admin all render from one table later.

**Tech Stack:** Rust 2024, serde_json, wiremock. All cargo runs on EVO via `./scripts/sync-evo.sh`.

**File-length constraints that shape this plan (CI ratchet):** `src/api/bots.rs` is AT its 1466 ceiling → the smoke validator moves OUT to a new module and the dead `validate_smoke_json` is deleted (net negative). `src/orchestrator/error_kind.rs` (229) would breach 300 with a guidance table → table goes in new `error_guidance.rs`. `src/observability/sentinels.rs` (296) would breach with ING-001 → its unit tests move to `tests/sentinels_test.rs` first.

---

### Task 1: The ingest ladder — `src/bot_client/ingest.rs`

**Files:** Create `src/bot_client/ingest.rs`; modify `src/bot_client/mod.rs` (add `pub mod ingest;`).

- [ ] **Step 1: Write the module with its tests** (sans-io, total function — spec Part 2):

Types: `IngestKind { Clean, SalvagedField, SalvagedRaw, Truncated }` with `Default = Clean`, `as_str()` (`"clean" | "salvaged_field" | "salvaged_raw" | "truncated"`); `Ingested { text: String, kind: IngestKind }`; `pub fn ingest_prose(bytes: &[u8]) -> Ingested`.

Ladder order: (1) truncate at `MAX_RESPONSE_BYTES` on a char boundary, remembering it; (2) JSON object → first non-empty string among `text`, `response` (Clean) then `output`, `content`, `message`, `answer` (SalvagedField) then OpenAI `choices[0].message.content` (SalvagedField); (3) bare JSON string → its content (SalvagedRaw); (4) anything else → lossy-UTF8 raw body (SalvagedRaw); (5) strip one wrapping ``` fence pair and a leading `<think>…</think>` block; (6) if truncation happened, kind = Truncated regardless of rung.

Tests: one per rung and field name; OpenAI envelope; fence + `<think>` stripping; truncation kind + length; JSON object with no known field falls to raw; empty body → empty text Clean; adversarial instruction-shaped prose comes through as inert text; `as_str` closed set.

- [ ] **Step 2:** `./scripts/sync-evo.sh check` then run `cargo test --lib ingest` on EVO. Expected: PASS.
- [ ] **Step 3:** Commit `feat(ingest): lenient prose normalisation ladder`.

### Task 2: Wire ingest into the text-only dispatch path

**Files:** Modify `src/bot_client/text_only.rs`.

- [ ] **Step 1:** Replace the strict `TextOnlyResponse` parse and the oversize reject with the ladder: read bytes (no size reject), `let ingested = ingest::ingest_prose(&bytes);`, return `DebateRoundResponse { response: ingested.text, ingest_kind: ingested.kind, ..none }`. (The `ingest_kind` field arrives in Task 3 — Tasks 2+3 land as one commit to keep the crate green, lesson 1.)
- [ ] **Step 2:** Update tests: `malformed_json_is_propagated` becomes `raw_text_body_is_salvaged` (asserts `response == "not json"` and `kind == SalvagedRaw`); add `response_field_accepted` (`{"response": "…"}` → Clean) and `oversize_body_truncates` (30 KB body → ok, kind Truncated, `len <= MAX_RESPONSE_BYTES`).

### Task 3: Wire ingest into the external dispatch path + carry `ingest_kind`

**Files:** Modify `src/bot_client/mod.rs` (struct + `send_debate_request`), `src/orchestrator/dispatch.rs:140`, `src/orchestrator/response_parser.rs:75,233,247`, `src/orchestrator/extraction.rs:417` (constructor sites gain `ingest_kind: IngestKind::Clean` or `Default::default()`).

- [ ] **Step 1:** `DebateRoundResponse` gains `#[serde(skip, default)] pub ingest_kind: ingest::IngestKind`.
- [ ] **Step 2:** `send_debate_request`: drop the oversize reject; try `serde_json::from_slice::<DebateRoundResponse>` first (structured fields survive; kind Clean); on failure, ladder the bytes into a response-only value. A non-2xx status remains an error (that is transport, not shape).
- [ ] **Step 3:** Tests in `mod.rs` dispatch_tests: external bot returning `{"text": "…"}` (text-only shape on the external path) now succeeds via salvage; raw prose body succeeds as SalvagedRaw; structured `{"response","challenge"}` keeps its challenge and reports Clean.
- [ ] **Step 4:** `./scripts/sync-evo.sh` full suite green (existing five_round/text_only flows must pass untouched). Commit `feat(ingest): both dispatch paths accept anything prose-shaped`.

### Task 4: Persist `ingest_kind`

**Files:** Create `migrations/20260702000001_responses_ingest_kind.sql` (`ALTER TABLE responses ADD COLUMN ingest_kind TEXT NULL;`); modify `src/db/queries_phase1.rs::insert_response_full` (new `ingest_kind: Option<&str>` param + column + bind); update every call site (grep `insert_response_full(`: `src/orchestrator/mod.rs:101`, `rounds/round0–4.rs`, `round3_legacy.rs`) passing `Some(response.ingest_kind.as_str())` where a bot response exists, `None` for abstention/carry-forward rows.

- [ ] **Step 1:** Migration + query + call sites (compiler-guided).
- [ ] **Step 2:** Extend one integration assertion in `tests/text_only_bot_flow.rs`: fetch a stored response row and assert `ingest_kind = 'clean'`.
- [ ] **Step 3:** Full suite on EVO. Commit `feat(ingest): persist ingest_kind per response`.

### Task 5: `auth` error kind + owner guidance table

**Files:** Modify `src/orchestrator/error_kind.rs` (new arm BEFORE the generic 4xx branch: status 401/403 → kind `"auth"`, detail `HTTP {status}`); create `src/orchestrator/error_guidance.rs`.

- [ ] **Step 1:** `error_guidance.rs`: `pub struct Guidance { pub description: &'static str, pub fix_hint: &'static str }` and `pub fn for_kind(kind: &str) -> Guidance` covering: `timeout`, `connection_refused`, `dns`, `tls`, `auth`, `http_5xx`, `http_4xx`, `json_parse`, `schema_missing_field`, `schema_invalid_type`, `schema_invalid_value`, `internal`, with an unknown-kind fallback. Plain English, owner-facing (spec Part 3; e.g. auth: "Your endpoint rejected the council's credentials." / "Check that your bot uses the exact token you registered."). Legacy shape kinds note they are historical post-ingest.
- [ ] **Step 2:** Tests: 401 and 403 classify as `auth` (update the existing `classifies_http_401` test); 404 stays `http_4xx`; every kind named in `error_kind.rs` has non-empty guidance (exhaustiveness list test); unknown kind falls back.
- [ ] **Step 3:** Wire `mod error_guidance;` in `src/orchestrator/mod.rs`. Full suite. Commit `feat(errors): auth kind + owner-facing guidance table`.

### Task 6: Lenient smoke validation (and bots.rs ratchet compliance)

**Files:** Create `src/api/bot_validation.rs`; modify `src/api/bots.rs` (delete dead `validate_smoke_json` at ~line 735, move `validate_smoke_json_for_round` out, import from the new module); modify `src/api/mod.rs` (module decl).

- [ ] **Step 1:** `bot_validation.rs`: `pub(crate) fn validate_smoke_json_for_round(json: &serde_json::Value, _round: i64) -> Result<(), String>` — serialise the value back to bytes, run `ingest_prose`, non-empty text = Ok; empty = Err("bot returned no readable prose — see /bots/guide for the expected response shape"). Unit tests: `{"text": …}` ok; `{"response": …}` ok; `{"output": …}` ok (salvaged); `{"unrelated": 1}` err; prose-bearing arbitrary JSON ok.
- [ ] **Step 2:** bots.rs shrinks (dead fn deleted, validator moved): verify `./scripts/check-file-length.sh` passes.
- [ ] **Step 3:** Full suite. Commit `feat(smoke): validator accepts anything prose-shaped`.

### Task 7: Sentinel ING-001

**Files:** Create `tests/sentinels_test.rs` (move the `#[cfg(test)]` module out of `src/observability/sentinels.rs` — public API only; keep `inventory_is_fresh` + `regen_inventory` in the integration file with `include_str!` path adjusted to `../docs/sentinels.md`); modify `src/observability/sentinels.rs` (add const + validator + array entry); regenerate `docs/sentinels.md`.

- [ ] **Step 1:** `ING_NEVER_REJECTS: Sentinel { id: "ING-001", statement: "Ingest is total: any response body yields a stored result with a closed-set kind, never a dispatch error." }`; `pub fn check_ingest_kind(kind: &str) -> Vec<Violation>` (closed set, mirroring EXT-001); `SENTINELS: [Sentinel; 5]`. Call `check_ingest_kind` + `log_violations("ingest", …)` from `ingest_prose`'s return path on `kind.as_str()` (guards future refactors that add a kind without inventorying it).
- [ ] **Step 2:** Regenerate the inventory (`cargo test --test sentinels_test regen_inventory -- --ignored` on EVO, scp back) — or hand-write matching bytes; the freshness test arbitrates.
- [ ] **Step 3:** `src/observability/sentinels.rs` under 300 lines (`./scripts/check-file-length.sh`). Full suite. Commit `feat(observability): ING-001 ingest sentinel`.

### Task 8: Ship the PR

- [ ] **Step 1:** `./scripts/sync-evo.sh` + `cargo fmt --check` + `cargo clippy --all-targets` (zero errors) on EVO; `./scripts/check-file-length.sh` locally.
- [ ] **Step 2:** Push `claude/lenient-ingest`, `gh pr create` (summary: spec Parts 2–3 Phase 1; note NOT shipped alone — rides with the next prod ship). Wait CI green, squash-merge `--delete-branch`.

---

**Self-review notes:** Spec coverage — Part 2 ladder rungs (T1), verbatim storage (unchanged: `response_json` stores what ingest returns; raw-bytes preservation beyond that deferred with the monitoring page, noted in PR), truncation-not-rejection (T1–T3), `ingest_kind` recording (T4), old shape-kinds unreachable (T2/T3 remove the reject paths; enum retained), ING-001 (T7); Part 3 auth kind + one table (T5). Type consistency — `IngestKind::as_str`, `Ingested`, `ingest_prose` named identically in T1–T7. No placeholders.
