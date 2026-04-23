# CLAUDE.md — LQ Bot Council Harness

## Quick Reference

| Key | Value |
|-----|-------|
| **Language** | Rust 2024 edition |
| **Framework** | Axum 0.8, Tokio |
| **Database** | SQLite via sqlx 0.8 |
| **Frontend** | SvelteKit (Svelte 5 runes, static adapter) — **served by Axum on EVO, not Vercel** |
| **Port** | 3100 |
| **Config** | `config/default.toml` + `APP__*` env vars + runtime-served `/api/config.json` |
| **Build/test host** | EVO X2 (remote Linux); CI also runs on GitHub-hosted Ubuntu |
| **GitHub repo** | `jamescockburn47/bot-council` |
| **Default branch** | `main` |
| **Prod URL** | `https://lqcouncil.com` — single origin, Axum serves both `/` (frontend) and `/api/*` (backend) |
| **LLM** | MiniMax-M2.7 via `https://api.minimax.io/v1/chat/completions` (OpenAI-compatible, Bearer auth). Local llama-server + Gemma remains available on EVO `:8086` as a rollback target but is not currently wired to any model route. |

## Public request chain

```
browser → Cloudflare edge (CDN, TLS, DNS apex CNAME flattened; NS: gloria+mitch.ns.cloudflare.com)
        → Cloudflare Tunnel (sovren-evo, 4× QUIC connections to London edges)
        → cloudflared on EVO (systemd unit: sovren-cloudflared.service)
        → http://localhost:3100 (Axum)
           ├─ /api/*            handlers
           ├─ /api/config.json  runtime config (Clerk pk_live_*, sentry env, release SHA)
           └─ /*                tower-http ServeDir + index.html SPA fallback
```

No Vercel in the path. The domain is still registered at Vercel (no cost, no reason to transfer), but DNS, TLS, and proxying are entirely Cloudflare.

## EVO SSH

The crate **does not build on Windows**; all `cargo` runs over SSH to EVO.

| | |
|---|---|
| **Host** | `james@100.90.66.54` (Tailscale) |
| **Alternate** | `james@10.0.0.2` (ethernet) / `james@192.168.1.230` (WiFi) |
| **User** | `james` — NOT `pi` |
| **Project path** | `~/bot-council` (scp'd source, NOT a git checkout) |
| **SSH key** | `C:/Users/James/.ssh/id_ed25519` |
| **Cargo env** | `source ~/.cargo/env` in every SSH session |
| **systemd units** | `bot-council.service`, `sovren-cloudflared.service` |
| **Env file** | `/etc/bot-council.env` (root:root 0600) |

## Deploy workflow — BINDING

### Build-host prerequisites

`frontend/.env.production` (gitignored) must exist on the dev box before `ship.sh`. Vite reads it during `npm run build` to bake `PUBLIC_SENTRY_DSN` and `PUBLIC_SENTRY_ENVIRONMENT` into the bundle. Without it, frontend Sentry silently no-ops — that's how PR #62 (Vercel retire) accidentally killed our browser telemetry until 2026-04-20. To set up a fresh checkout:

```bash
cat > frontend/.env.production <<'EOF'
PUBLIC_SENTRY_DSN=<dsn from /etc/bot-council.env on EVO: APP__SENTRY__DSN>
PUBLIC_SENTRY_ENVIRONMENT=prod
EOF
```

### One command to ship

```bash
./scripts/ship.sh
```

Does all seven of:
1. Preflight — refuses dirty tree, non-main branch, or unreachable EVO
2. Env-file preflight on EVO — required `APP__*` keys present + non-empty
3. `npm ci && npm run build` locally (svelte-check + vite build)
4. scp src + tests + config + migrations + Cargo.toml/lock + frontend/build to EVO
5. On EVO: save current binary as `.prev`, `cargo build --release`, write `SENTRY_RELEASE=<sha>`, `systemctl restart`
6. 30-second health poll against `http://127.0.0.1:3100/api/health`
7. Public smoke `curl https://lqcouncil.com/api/health` returns JSON `{"status":"ok"}`

Exits non-zero at the failing stage if any step fails. `.last-known-good-sha` written on success.

### One command to roll back

```bash
./scripts/rollback.sh
```

Stops bot-council, moves current binary aside as `.broken.<timestamp>`, promotes `.prev` back to live, restarts, health-polls. ~10 seconds. Never rebuilds. Prints the last-known-good SHA so the operator can `git reset --hard <sha>` to match source with what's running.

### Dev iteration on EVO without a full deploy

`scripts/sync-evo.sh` remains for cargo check/test/build/run on EVO without the full ship cycle:

```bash
./scripts/sync-evo.sh        # cargo test (default)
./scripts/sync-evo.sh check  # cargo check --tests
./scripts/sync-evo.sh build  # cargo build --release
./scripts/sync-evo.sh run    # cargo run
```

`sync-evo.sh restart` is deprecated — use `ship.sh` instead.

### Installing/updating the systemd unit file

Unit file changes (`deploy/bot-council.service`) aren't covered by `ship.sh`. Install manually:

```bash
scp -i ~/.ssh/id_ed25519 deploy/bot-council.service james@100.90.66.54:/tmp/
ssh ... "sudo cp /tmp/bot-council.service /etc/systemd/system/ && sudo systemctl daemon-reload && systemd-analyze verify /etc/systemd/system/bot-council.service && sudo systemctl restart bot-council"
```

Check `systemd-analyze verify` output for any unknown-lvalue warnings on OUR unit (other units on EVO may have their own issues that show in the scan; ignore those).

## CI — BINDING

GitHub Actions on every PR and push to main. Workflow: `.github/workflows/ci.yml`.

**Backend job**: `cargo fmt --check`, `cargo clippy --all-targets` (warnings permitted for now; tighten later), `cargo test --all`. Runs on ubuntu-latest with cargo cache via `Swatinem/rust-cache`. ~5 min cold, <1 min warm.

**Frontend job**: `npm ci`, `npm run build` (chains `svelte-kit sync` + `svelte-check` + `vite build`). ~40 seconds.

Both jobs must pass before merging to main (enable branch protection in GitHub Settings → Branches → main if not already). `concurrency.cancel-in-progress` kills stale runs on the same branch when a new push lands.

## Branch Hygiene — BINDING

```bash
gh pr list --state open  # check for conflicting work first
git fetch origin
git switch -c claude/<topic> origin/main
```

Never implement on `master` or stale feature branches. One PR per logical change. Squash-merge with `--delete-branch`. Many small PRs over one large one.

Multiple git worktrees exist on the dev machine (historical). The one on `main` is `..../worktrees/reverent-goldwasser` — use that for running `ship.sh`. Worktree cleanup via `scripts/branch-cleanup.ps1`.

## GitHub Workflow — BINDING

### Creating a commit

HEREDOC for multi-line messages:

```bash
git commit -m "$(cat <<'EOF'
feat: short imperative subject line

Body explains why. Wrap at 72 chars.
Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

### Creating a PR

```bash
gh pr create --base main --head claude/<branch> --title "..." --body "$(cat <<'EOF'
## Summary
...
## Test plan
- [x] cargo test
- [x] frontend build
- [x] ship.sh (after merge)
🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

### Pre-commit self-checks

1. `./scripts/sync-evo.sh` green (backend tests)
2. `cd frontend && npm run build` green (when frontend touched)
3. No stray `*.bak`, `*.step1`, `*.final`, or misplaced `*.sql` in working tree
4. `.gitattributes` enforces LF — do not override with Windows `core.autocrlf=true` (sqlx migration checksums are content-sensitive, CRLF/LF drift has bitten this project)

### Merge hygiene

- Prefer many small PRs over one large one
- Never force-push to `main`
- Wait for CI green before merging (required by branch protection once enabled)

## Coding Standards — BINDING

- Max 300 lines per file. Split before adding.
- One file, one job. Single responsibility.
- No `unwrap()` in production paths. `unwrap()` allowed in `#[cfg(test)]`.
- No `.ok()` on `Result` without `// intentional: [reason]` comment.
- Newtype wrappers for IDs: `DebateId(String)`, `BotId(String)`.
- Enums with serde derive for fixed values.
- All config in `config.rs`. Zero `std::env` outside config.
- Repository pattern: handlers call `db::queries`, never raw SQL.
- `thiserror` for domain errors, `anyhow` at binary boundary only.
- Tracing with structured fields for all error logging. Never log bearer tokens, raw JWT claims, or AES keys.
- `join_all` for concurrent independent operations.
- Integration tests via `tower::ServiceExt::oneshot` with in-memory SQLite.
- `///` doc comments on all public items.
- Atomic commits. One logical change per commit.

## Architecture

Standalone Rust/Axum service on port 3100. SvelteKit static build served by the same process via `tower-http::ServeDir` with `index.html` as the SPA fallback. SQLite for persistence. Tokio background tasks run debates asynchronously. Clerk JWT + JWKS verification for user auth. AES-256-GCM for bot token storage. In-app `admins` table for runtime role management. Cloudflare Tunnel for public ingress; no port-forwarding at the network edge.

Full topology in [ARCHITECTURE.md](ARCHITECTURE.md).

## API endpoints (current)

All routes mounted under `/api/*` in production. Tests use the un-prefixed routes directly against `api::router()`.

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | /api/health | public | Health check |
| GET | /api/config.json | public | Frontend runtime config (Clerk pk_*, sentry env, release SHA) |
| GET | /api/diag/health | public (alias) | Same as /api/health |
| GET | /api/diag/models | RequireAdmin | Effective model routing (analysis + final_synthesis URLs/models) |
| GET | /api/me | RequireAuth | Current user identity + role |
| GET | /api/bots | RequireAuth | Admin: all. Participant: active only |
| POST | /api/bots | RequireAuth | Admin → active. Participant → pending. Accepts `bot_kind: "external" \| "text_only"` (defaults to `external`). UI submits text-only. |
| GET | /api/bots/my-submissions | RequireAuth | Requires Clerk user_id |
| GET | /api/bots/schema | RequireAuth | Legacy-compat bot schema shim |
| POST | /api/bots/validate | RequireAuth | Validate a bot submission without creating |
| GET | /api/bots/{id}/history | RequireAuth | Per-bot response history |
| GET | /api/bots/{id}/analytics | RequireAuth | Per-bot performance metrics |
| PATCH | /api/bots/{id}/approve\|reject\|deactivate\|reactivate | RequireAdmin | State transitions |
| PATCH | /api/bots/{id}/test | RequireAdmin | Manual bot smoke-test |
| GET | /api/debates | RequireAuth | List debates |
| POST | /api/debates | RequireAdmin | Create + run (synchronous preflight ~100s for 5 bots) |
| GET | /api/debates/{id} | RequireAuth | Debate detail |
| GET | /api/debates/{id}/transcript | RequireAuth | Round-by-round transcript. TranscriptEntry carries optional `extraction_metadata` keyed by field (`challenge`, `position_change`) with `{source, quote}` provenance for text-only bots. |
| GET | /api/debates/{id}/synthesis | RequireAuth | Final synthesis JSON |
| GET | /api/debates/{id}/stream | RequireAuth (header OR `?token=`) | SSE live stream |
| PATCH | /api/debates/{id}/archive | RequireAdmin | Body `{"archived": bool}` — soft archive/unarchive. Sets/clears `archived_at`. Archived debates hide from the default list; `?archived=true` surfaces them. |
| DELETE | /api/debates/{id} | RequireAdmin | Permanent cascade delete (transcript, responses, analyses, syntheses, debate_bots, broadcast channel). Not reversible. |
| GET | /api/admins | RequireAdmin | List admins |
| POST | /api/admins | RequireAdmin | Promote a user_id |
| DELETE | /api/admins/{user_id} | RequireAdmin | Demote (cannot demote self) |
| GET | /api/users | RequireAdmin | List signed-in users with `is_admin` + email |

## Auth model

- **Anonymous**: `/api/health`, `/api/config.json`, `/api/diag/health`, and all static files.
- **Participant** (Clerk user not in `admins` table): read debates, submit bots → pending, view own submissions.
- **Admin**: everything.
- **Admin bearer token** (`APP__AUTH__ADMIN_TOKEN`): CLI / emergency / bootstrap-first-admin path. Sends `Authorization: Bearer <token>` — identity is `Admin { user_id: None, source: BearerToken }`.
- **Promotion**: `POST /api/admins` with a Clerk user_id. First admin bootstrapped by bearer token before the user has admin rights via any other path. **Already done** — 4 admins in DB as of 2026-04-18.

## Current state (2026-04-23)

**Live architecture**: single-origin EVO fronted by Cloudflare Tunnel. Vercel fully retired (both the bot-council frontend project and the lqcouncil-api-proxy project removed).

**Bot contract (unified)**: every bot receives a prompt and returns prose. Structured fields (`challenge`, `position_change`, `steelman`, `crux_engagement`) are extracted from prose by MiniMax with source-quote verification — whether or not the bot authored them on the wire. `bot_kind` still exists as a column with values `external` and `text_only`, but it no longer gates smoke validation or extraction; the distinction is purely a note on which wire-shape the bot expects (external = full `DebateRoundRequest`, text_only = flat `{prompt, session_id} → {text}`). Both wire shapes continue to work.
- Token is optional at submission; NULL token means "dispatch sends no Authorization header". Public bots may still set one for LLM-budget protection.
- Endpoint URL accepts `https://…` or `http://localhost*` / `http://127.0.0.1*` / `http://[::1]*`. Public endpoints must use HTTPS.
- Smoke validator checks a single thing: the bot returned non-empty prose in `response` or `text`. No per-round structured schema enforcement.
- Smoke per-request timeout is 180s (was 60s) so tool-heavy research fits.
- Preflight runs the smoke test with whatever token is stored (or none); failures other than reachability no longer exist.

Design: `docs/superpowers/specs/2026-04-23-unified-bot-contract-design.md` (extends `2026-04-22-text-only-bot-mode-design.md`; the strict external path described there is retired).

**LLM routing**: all analyser + final-synthesis calls hit **MiniMax-M2.7 at `https://api.minimax.io`** (OpenAI-compatible API, Bearer auth). Env file has `APP__MODELS__ANALYSIS_BASE_URL`, `APP__MODELS__FINAL_SYNTHESIS_BASE_URL`, and `APP__MODELS__MINIMAX_BASE_URL` all pointing at api.minimax.io, and `APP__MODELS__MINIMAX_API_KEY` set. `APP__MODELS__FINAL_SYNTHESIS_WARMUP_ENABLED=false` (not needed for a hosted API). The historic Anthropic + CometAPI keys zeroed 2026-04-20 remain zeroed. `config/default.toml` defaults still point at the local llama-server for analysis + final_synthesis, so if MiniMax is down the rollback is "unset the `APP__MODELS__*_BASE_URL` overrides, restart bot-council, llama-server picks up the traffic" — provided the local llama-server on `:8086` is running.

**Resynth / cleanup CLI**: `bot-council resynthesise` rebuilds the stored synthesis for one or all concluded debates by re-running the analyser + synthesiser against the existing transcript. Used after a synthesis-prompt change to refresh historical debate headlines without re-running the full debate. `bot-council test-cleanup` sweeps auto-deletable test debates. Both source `/etc/bot-council.env` to pick up admin-level model routing — see `/home/james/resynth-launch.sh` on EVO for the standard invocation (default `--throttle-ms 2000`; use `500` on Pro-tier MiniMax).

**CI**: GitHub Actions enforces backend (fmt/clippy/test) and frontend (svelte-check/build). Enable branch protection in GitHub Settings → Branches to require CI pass before merging.

**Hardening live**:
- `src/lib.rs`: JWKS startup-wait retries with exponential backoff (1s/2s/4s/8s/16s × ~91s worst case) then `bail!`. Previously a Clerk DNS hiccup at boot gave a 10-minute degraded-auth window.
- `deploy/bot-council.service`: `StartLimitBurst=5`/`StartLimitIntervalSec=300` (in [Unit]) caps restart loops; `ExecStartPre=/usr/bin/test -s /etc/bot-council.env` refuses empty configs; `TimeoutStartSec=120` caps startup hangs.
- Synthesis schema (`src/synthesiser/schema.rs`): every top-level field and every `headline` field carries `#[serde(default)]`. MiniMax occasionally drops one field on shorter transcripts; without defaults the typed parse would fail and the whole synthesis would fall through to an empty-template salvage. With defaults, only the dropped section is empty.

## Specs + plans

- Hardening plan: `C:\Users\James\.claude\plans\this-has-become-buggy-linked-seal.md`
- Harness design: `docs/superpowers/specs/2026-04-15-bot-council-harness-design.md`
- Clerk auth design: `docs/superpowers/specs/2026-04-16-clerk-auth-and-bot-submission-cleanup-design.md`
- Text-only bot mode design: `docs/superpowers/specs/2026-04-22-text-only-bot-mode-design.md`
- Text-only bot mode Phase 1 plan: `docs/superpowers/plans/2026-04-22-text-only-bot-mode.md`

## Operational lessons — BINDING

Things learned the hard way. Do not rediscover them.

1. **`cargo test` builds the whole crate.** A type error in one handler blocks tests in unrelated modules. Never leave an intentional compile break longer than one task.
2. **`cargo check --lib` vs `--tests`** — `--tests` builds test binaries too. Use `--lib` when iterating on library code; `--tests` once you're ready to run tests.
3. **`common::admin_auth` pattern**. Every integration test must wrap its request builder:
   ```rust
   let req = common::admin_auth(Request::builder().method("POST").uri("..."))
       .body(Body::from(...)).unwrap();
   ```
4. **Bot token storage is AES-256-GCM ciphertext only.** Stored in `bots.token_ciphertext BLOB`. Legacy `token_hash`/`active` columns were dropped in migration `20260416000003_drop_legacy_bot_columns.sql`. Do NOT re-introduce them.
5. **EventSource + auth.** `/api/debates/{id}/stream` cannot use the Authorization header from the browser. Always use `debateStreamUrl(id, token)` from the frontend; backend accepts `?token=` alongside the header.
6. **Svelte 5 runes.** Use `$state`, `$derived`, `$effect`, `$props()`, `$effect.root()`. `let x = $state(...)` not `let x: T = $state<T>(...)`. Read existing pages before adding new ones.
7. **`$app/stores` vs `$app/state`** — neither is safe for `page` reads in this adapter-static + Svelte 5 build. `$app/state` breaks hydration silently; the `page` store from `$app/stores` is null for a microtask on first render and the store-as-signal read crashes with *"Cannot read properties of null (reading 'r')"*. Use `afterNavigate` from `$app/navigation` + a `window.location.pathname` snapshot. Ref: PR #72. Enforced by CI (see `.github/workflows/ci.yml` "Forbid known-broken imports").
8. **`onDestroy` from 'svelte' is banned in this repo.** Current Svelte 5 + Vite splitting routes it to the SSR renderer's context (`Mt.r.on_destroy`), whose `Mt` is null in CSR — same null-signal crash as #7. Use `$effect(() => () => cleanup())` instead. Ref: PR #73. Enforced by CI.
9. **Subagent rigour.** For backend Rust: full implementer + spec + code review on substantive tasks; manual verification on mechanical ones (migrations, config tweaks, extractor renames). Write tests before delegating.
10. **Always check for open PRs first.** `gh pr list --state open` before starting any branch. Parallel PRs touching the same files cause conflicts and risk dropping fixes.
11. **User has all project access.** Do not ask the user to run commands you can run yourself (`gh pr merge`, `git push`, `scp` to EVO). Autonomous end-to-end execution expected.
12. **Never edit migrations that have been applied to prod.** sqlx's `migrate!()` macro embeds file bytes at compile time and checksums them against `_sqlx_migrations`. Any byte-level change (including CRLF↔LF conversion!) makes the binary refuse to boot. `.gitattributes` enforces LF repo-wide; never override with `core.autocrlf=true`.
13. **EVO has no git repo.** `~/bot-council/` is scp'd source. Rollback is binary-swap (`.prev`), not git-based. Don't design tooling that assumes git on EVO.
14. **Deploy-driven config drift.** `/etc/bot-council.env` has been modified out-of-band multiple times (CometAPI/Anthropic keys added by an earlier session without a git trail; Gemma→MiniMax route switch via env override). Probe `sudo grep '^APP__' /etc/bot-council.env` at the start of any config-touching session to see what's actually live — don't trust `config/default.toml` as a source of truth for runtime routing.
15. **cloudflared CLI is zone-scoped.** `cloudflared tunnel route dns <tunnel> <host>` adds the CNAME in whatever zone `cert.pem` is authenticated for (`sovren.xyz` in our case). For a different zone (`lqcouncil.com`), add the CNAME manually in the Cloudflare dashboard. We have one leftover `lqcouncil.com.sovren.xyz` orphan record from this quirk.
16. **After any synthesis-prompt change, run the resynth batch.** The synthesis prompt in `src/synthesiser/mod.rs::build_synthesis_prompt` is used by `bot-council resynthesise` too; existing concluded debates won't pick up prompt fixes until you rerun them. Ship first, then `ssh evo "bash /home/james/resynth-launch.sh"` (or `bot-council resynthesise <debate_id>` to target one). `--throttle-ms 500` is safe on MiniMax Pro; default `2000` for free-tier.
17. **Extractor provenance must not lie.** `src/orchestrator/extraction.rs::extract_if_needed` returns `FieldProvenance { source: "extracted" }` only when every field's MiniMax-declared source quote passes `extractor::verify::quote_is_substring_of` AND the typed-struct `serde_json::from_value` deserialises cleanly. Any shape-mismatch or quote-fabrication downgrades to `source: "extraction_failed"`. Never skip the downgrade path — a lying "extracted" label with an unpatched field silently breaks the anti-hallucination story the feature was built for.
18. **Clint's `lqc_*` tools still speak the legacy `/debate` contract.** `lqc_validate_bot` and `lqc_dry_run_debate` (in the `~/clawdbot` repo, not this one) have NOT been updated for text-only mode. Until they are, operators onboarding a text-only bot must use the `/bots/submit` UI and the approval smoke — the Clint-driven validation path will reject or mis-diagnose text-only bots.
