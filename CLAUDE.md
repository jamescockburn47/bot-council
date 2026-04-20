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
| **LLM** | local `llama-server` (llama.cpp) on EVO `:8086`, model `gemma-4-31B-it-Q4_K_M.gguf` |

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
| POST | /api/bots | RequireAuth | Admin → active. Participant → pending |
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
| GET | /api/debates/{id}/transcript | RequireAuth | Round-by-round transcript |
| GET | /api/debates/{id}/synthesis | RequireAuth | Final synthesis JSON |
| GET | /api/debates/{id}/stream | RequireAuth (header OR `?token=`) | SSE live stream |
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

## Current state (2026-04-20)

**Live architecture**: single-origin EVO fronted by Cloudflare Tunnel. Vercel fully retired (both the bot-council frontend project and the lqcouncil-api-proxy project removed).

**Local LLM**: all inference hits `http://127.0.0.1:8086` (gemma-4-31B via llama.cpp's llama-server). Stray Anthropic + CometAPI keys found in `/etc/bot-council.env` were zeroed out on 2026-04-20; revoke at provider consoles if not already done.

**CI**: GitHub Actions enforces backend (fmt/clippy/test) and frontend (svelte-check/build). Enable branch protection in GitHub Settings → Branches to require CI pass before merging.

**Hardening live**:
- `src/lib.rs`: JWKS startup-wait retries with exponential backoff (1s/2s/4s/8s/16s × ~91s worst case) then `bail!`. Previously a Clerk DNS hiccup at boot gave a 10-minute degraded-auth window.
- `deploy/bot-council.service`: `StartLimitBurst=5`/`StartLimitIntervalSec=300` (in [Unit]) caps restart loops; `ExecStartPre=/usr/bin/test -s /etc/bot-council.env` refuses empty configs; `TimeoutStartSec=120` caps startup hangs.

## Specs + plans

- Hardening plan: `C:\Users\James\.claude\plans\this-has-become-buggy-linked-seal.md`
- Harness design: `docs/superpowers/specs/2026-04-15-bot-council-harness-design.md`
- Clerk auth design: `docs/superpowers/specs/2026-04-16-clerk-auth-and-bot-submission-cleanup-design.md`

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
7. **`$app/stores` vs `$app/state`** — use `$app/stores` for static-adapter builds. `$app/state` mixed with adapter-static kills hydration silently. Regressed twice; PR #55 is the latest fix.
8. **Subagent rigour.** For backend Rust: full implementer + spec + code review on substantive tasks; manual verification on mechanical ones (migrations, config tweaks, extractor renames). Write tests before delegating.
9. **Always check for open PRs first.** `gh pr list --state open` before starting any branch. Parallel PRs touching the same files cause conflicts and risk dropping fixes.
10. **User has all project access.** Do not ask the user to run commands you can run yourself (`gh pr merge`, `git push`, `scp` to EVO). Autonomous end-to-end execution expected.
11. **Never edit migrations that have been applied to prod.** sqlx's `migrate!()` macro embeds file bytes at compile time and checksums them against `_sqlx_migrations`. Any byte-level change (including CRLF↔LF conversion!) makes the binary refuse to boot. `.gitattributes` enforces LF repo-wide; never override with `core.autocrlf=true`.
12. **EVO has no git repo.** `~/bot-council/` is scp'd source. Rollback is binary-swap (`.prev`), not git-based. Don't design tooling that assumes git on EVO.
13. **Deploy-driven config drift.** An earlier session installed cloud API keys in `/etc/bot-council.env` (CometAPI + Anthropic) without tracking it in git. Probe `sudo grep '^APP__' /etc/bot-council.env` at the start of any config-touching session to see what's actually live. Memory file `project_local_models.md` captures the intent.
14. **cloudflared CLI is zone-scoped.** `cloudflared tunnel route dns <tunnel> <host>` adds the CNAME in whatever zone `cert.pem` is authenticated for (`sovren.xyz` in our case). For a different zone (`lqcouncil.com`), add the CNAME manually in the Cloudflare dashboard. We have one leftover `lqcouncil.com.sovren.xyz` orphan record from this quirk.
