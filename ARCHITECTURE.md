# ARCHITECTURE.md — LQ Bot Council

Forensic description of how LQ Council is built, served, and interacts
end-to-end. Every claim here was verified against the code at commit
`deaf2f1` (PR #34, 2026-04-17) and against live probes of the production
hosts on 2026-04-18. Treat this file — not stale parts of `CLAUDE.md` — as the
canonical architecture reference. When this file drifts, update it with the
change; do not let new docs go out of sync.

## 1. Deployment topology

This section was verified by live probe on 2026-04-18. The older documentation
in CLAUDE.md claimed the backend was fronted directly by Cloudflare Tunnel;
**that is not correct**. Actual production path:

```
 Browser
   │
   │  https://lqcouncil.com/...  (static SvelteKit SPA)
   ▼
 Vercel project: bot-council
   (SvelteKit bundle on CDN, Clerk SDK in browser)

 Browser (after Clerk login, XHR / SSE with Clerk JWT)
   │
   │  https://api.lqcouncil.com/...
   ▼
 Vercel project: lqcouncil-api-proxy
   (vercel.json rewrite: /(.*) → https://james-nucbox-evo-x2.taila41c86.ts.net/$1)
   │
   ▼
 Tailscale Funnel on EVO
   https://james-nucbox-evo-x2.taila41c86.ts.net → http://127.0.0.1:3100
   │
   ▼
 EVO X2 (Ubuntu 24.04, Tailscale 100.90.66.54)
   bot-council binary under systemd (bot-council.service, active+enabled)
   axum on 0.0.0.0:3100
   SQLite ~/bot-council/data/council.db (WAL)
   /etc/bot-council.env (root:root 0600, sourced by systemd unit)
```

**Ingress history (for future-me).** Before 2026-04-18, the Vercel proxy
rewrote to `https://council.sovren.xyz`, which was served by a cloudflared
tunnel named `sovren-evo` (managed by `sovren-cloudflared.service`). That
tunnel belonged to the Sovren project but had a `:3100` ingress entry, so
LQ Council's production path depended on it despite the misleading name.
On 2026-04-18 the Vercel proxy was re-pointed at the Tailscale Funnel via
`vercel.json` + `vercel alias set`, and `sovren-cloudflared.service` was
then disabled. `council.sovren.xyz` now returns 530 and is unused by LQ
Council. If the cloudflared tunnel is ever re-enabled for another project,
**do not re-add a `:3100` ingress** — LQ Council has a clean dedicated
path now.

**Probed 2026-04-18:**
- `GET https://api.lqcouncil.com/health` → 200 `{"status":"ok"}` (via Vercel
  proxy; `Server: Vercel`, `X-Vercel-Id: lhr1::…`, CORS headers from backend).
- `GET https://api.lqcouncil.com/me` → 401 (expected; no token).
- `GET https://lqcouncil.com/` → 200.
- `GET https://james-nucbox-evo-x2.taila41c86.ts.net/health` → 200 (Tailscale
  Funnel — now the production ingress).
- `GET http://localhost:3100/health` on EVO → 200.
- `GET https://council.sovren.xyz/health` → 530 (Sovren tunnel disabled
  2026-04-18 after the proxy cutover; route decommissioned).

**Runtime state warnings (caught during probe):**
- `systemctl is-active bot-council` = **inactive** on 2026-04-18. The running
  process is a manual `./target/release/bot-council` (pid 2382046) started
  ~4 h before the probe. Not auto-restart-safe. See §3.9 for remediation.
- `/etc/bot-council.env` has auth fields but **no `APP__MODELS__MINIMAX_API_KEY`
  or `APP__MODELS__OPUS_API_KEY`.** A debate created today would fail at the
  first LLM call. See §3.8 and INTEGRATIONS.md.
- Earlier journal entries show JWKS fetch failures against
  `https://clerk.lqcouncil.com/.well-known/jwks.json`. Verify that the Clerk
  Frontend API domain resolves before claiming auth is healthy.
- Tailscale health output on EVO: "Tailscale can't reach the configured DNS
  servers." Non-fatal but should be fixed — LAN DNS resolution via Tailscale
  will degrade.

## 2. Repository layout

```
bot-council/                       (repo root)
├── Cargo.toml                     single binary target `bot-council`
├── config/default.toml            all defaults; env overrides via APP__*
├── migrations/                    sqlx migrations, applied on boot
├── src/                           Rust backend (details §3)
│   ├── main.rs                    tokio entry; binds :3100
│   ├── lib.rs                     build_app(): state + router
│   ├── api/                       HTTP handlers + auth extractors
│   ├── auth/ (within api/)        Clerk JWKS + bearer auth
│   ├── db/                        sqlx pool + queries + models
│   ├── orchestrator/              debate round loop, state machine
│   ├── bot_client/                HTTP client to bot /debate endpoints
│   └── config.rs                  Settings struct + validation
├── frontend/                      SvelteKit SPA (details §4)
│   ├── svelte.config.js           adapter-static, SPA fallback
│   ├── vercel.json                one rewrite rule: /(.*) → /index.html
│   ├── src/lib/auth/clerk.ts      Clerk singleton, 12 s timeout (PR #29)
│   ├── src/lib/api/client.ts      fetch wrapper + SSE URL builder
│   └── src/routes/                pages (see §4.4)
├── scripts/sync-evo.sh            scp + cargo + systemctl restart helper
├── docs/                          specs, plans, deploy runbooks
└── ARCHITECTURE.md                this file
```

## 3. Backend

### 3.1 Process & startup

- Entry point: [`src/main.rs:4-15`](src/main.rs:4). `#[tokio::main]` (default
  multi-threaded runtime), `tracing_subscriber::fmt()` with `EnvFilter` from
  `RUST_LOG` (default `info`), `TcpListener::bind("0.0.0.0:3100")`.
- App construction: [`src/lib.rs:16-45`](src/lib.rs:16) via `build_app()`:
  1. Load `Settings` from `config/default.toml` + `APP__*` env vars
     ([`src/config.rs:79-89`](src/config.rs:79)).
  2. Open sqlite pool (max 5 conns, WAL, busy_timeout 5 s, FK on)
     ([`src/db/mod.rs:17-28`](src/db/mod.rs:17)). Creates the DB file and parent
     dir on first run.
  3. Run embedded sqlx migrations from `./migrations`.
  4. Parse `APP__AUTH__BOT_TOKEN_KEY` (64-char hex → 32 bytes) into a
     `BotTokenKey` newtype that zeroises on drop (PR #24).
  5. Create `JwksCache`, seed from `APP__AUTH__CLERK_ISSUER`, spawn a background
     refresh loop every 600 s.
  6. Build `reqwest::Client` with retry middleware.
  7. Assemble `AppState`; build router; return.

### 3.2 Router & middleware

[`src/api/mod.rs:39-60`](src/api/mod.rs:39). Axum `Router::new()` with explicit
routes. A single CORS layer is applied globally: permissive if
`server.cors_origins` is empty (dev), otherwise restrictive to the configured
list with methods `GET POST PATCH DELETE`. No tower logging/tracing layer;
structured tracing happens inside handlers.

Route groups (auth requirement in parens):

| Group | Routes |
|---|---|
| Public | `GET /health` |
| `RequireAuth` | `GET /me`, `GET /bots`, `POST /bots`, `GET /bots/my-submissions`, `GET /debates`, `GET /debates/{id}`, `GET /debates/{id}/transcript`, `GET /debates/{id}/synthesis`, `GET /debates/{id}/stream`, `GET /users` |
| `RequireAdmin` | `PATCH /bots/{id}/{approve\|reject\|deactivate\|reactivate}`, `POST /debates`, `GET/POST /admins`, `DELETE /admins/{user_id}` |

### 3.3 Auth pipeline

File: [`src/api/auth.rs`](src/api/auth.rs).

`authenticate()` ([`auth.rs:100-136`](src/api/auth.rs:100)) is the single
entry point used by all three extractors (`RequireAuth`, `RequireAdmin`,
bare `AuthIdentity`). Flow:

1. Extract token from `Authorization: Bearer …` header; if absent, try
   `?token=` query param (percent-decoded) for SSE.
2. If `APP__AUTH__ADMIN_TOKEN` is non-empty and matches exactly, return
   `AuthIdentity::Admin { user_id: None, source: AuthSource::BearerToken }`.
   This is the bootstrap-first-admin path and the emergency/CLI path.
3. Otherwise, if `APP__AUTH__CLERK_ISSUER` is set, run `verify_clerk_jwt()`:
   decode header → look up JWK by `kid` in the cache → RS256 verify with
   issuer check and 30 s leeway → extract `sub` as Clerk `user_id` →
   best-effort upsert into `seen_users` (never breaks auth) → check `admins`
   table → return `AuthIdentity::Admin{…}` or `::Participant{user_id}`.

Failure modes:

- No token → 401.
- Bearer mismatch and no Clerk issuer → 401.
- JWT invalid (signature, issuer, expiry) → 401.
- JWKS cache not yet populated and bearer didn't match → 500 "JWKS not yet
  loaded".
- Participant on admin-only route → 403.

### 3.4 SSE (`/debates/{id}/stream`)

File: [`src/api/stream.rs:26-77`](src/api/stream.rs:26).

- Auth via standard `AuthIdentity` extractor (header or `?token=`).
- Reject with 409 if the debate row is already in a terminal state
  (`complete | cancelled | failed`) — use REST for finished debates.
- Subscribe to a per-debate `tokio::sync::broadcast::Sender<DebateEvent>`
  stored in `AppState.debate_streams: DashMap<String, …>` (capacity 64,
  created in `create_debate` at [`src/api/debates.rs:77`](src/api/debates.rs:77),
  removed 60 s after terminal state).
- Map each event to an SSE frame (`event: <type>\ndata: <json>\n\n`).
- Merge a keepalive `:keepalive\n\n` every 30 s so Cloudflare doesn't idle-close
  the tunnel.
- Return with `Content-Type: text/event-stream`, `Cache-Control: no-cache`.

Event types defined in [`src/api/events.rs:6-65`](src/api/events.rs:6):
`debate:started`, `round:started`, `response:received`, `round:completed`,
`synthesis:started`, `synthesis:completed`, `debate:completed`, `debate:failed`.

### 3.5 Bot submission + smoke test

File: [`src/api/bots.rs`](src/api/bots.rs).

- `POST /bots` ([`bots.rs:31-68`](src/api/bots.rs:31)) validates name,
  endpoint URL (`https://` in release builds; `http://localhost|127.0.0.1`
  also allowed in debug), and token; encrypts the token with AES-256-GCM
  using `BotTokenKey`; status = `active` for admins, `pending` for others.
- Token ciphertext layout: `nonce(12) || ciphertext || tag(16)` as a BLOB
  ([`src/api/bot_token_crypto.rs:66-76`](src/api/bot_token_crypto.rs:66)).
- `PATCH /bots/{id}/approve` ([`bots.rs:176-207`](src/api/bots.rs:176))
  runs `smoke_test_bot()`: decrypt token, POST a dummy
  `DebateRoundRequest` with a 30 s timeout, verify HTTP 2xx and JSON with
  a `response: string` field. Errors are run through
  `classify_smoke_test_error()` ([`bots.rs:88-108`](src/api/bots.rs:88)) to
  produce a human-readable `rejection_reason` (DNS, connection, TLS, 401/403,
  HTTP status, JSON/missing field).
- State machine: `pending → active | smoke_test_failed | rejected`;
  `active ↔ inactive`.
- The legacy `token_hash` and `active` columns were dropped in migration
  `20260416000003_drop_legacy_bot_columns.sql` (PR #25). The repository layer
  does not touch them.

### 3.6 Debate execution

File: [`src/orchestrator/multi_round.rs`](src/orchestrator/multi_round.rs).

- `POST /debates` ([`src/api/debates.rs:15-123`](src/api/debates.rs:15))
  inserts the debate, creates the broadcast channel, and `tokio::spawn`s
  `run_multi_round_debate()`.
- Five rounds: `round_0` Blind Formation → `round_1` Anonymous Distribution →
  `round_2` Structured Rebuttal → `round_3` Cross-Examination → `round_4`
  Final Position → Synthesis.
- Per-bot HTTP call: [`src/bot_client/mod.rs:167-193`](src/bot_client/mod.rs:167).
  Per-bot `tokio::time::timeout(default 300 s)`. Bearer auth with the decrypted
  token. Up to `debate.max_retries` (default 2) on 5xx.
- Quorum: minimum `debate.quorum` bots (default 3) must respond per round,
  else debate → `failed` and a `DebateFailed { reason }` event is emitted.
- Status column transitions: `created → round_N → synthesis → complete | failed`.
- State machine supports resumption via `state_machine::find_resume_point()`.

### 3.7 Database

- Config default: `sqlite:data/council.db?mode=rwc`
  ([`config/default.toml:7`](config/default.toml:7)), overridable by
  `APP__DATABASE__URL`.
- Migrations (numbered, applied in order on boot):
  1. `20260415000001_init.sql` — bots, debates, debate_bots, responses, peer_scores.
  2. `20260415000002_phase1.sql` — rounds, analyses, pairings, syntheses, role_history.
  3. `20260415000003_phase1_5a.sql` — bot workflow columns (status, submitted_by, etc.).
  4. `20260415000004_citation_check.sql` — citation_check JSON on syntheses.
  5. `20260416000001_bot_submission_cleanup.sql` — `token_ciphertext BLOB`, `rejection_reason`.
  6. `20260416000002_admin_registry.sql` — `admins`, `seen_users`.
  7. `20260416000003_drop_legacy_bot_columns.sql` — drops `token_hash`, `active`.
- Pool: max 5 connections; WAL, `synchronous=NORMAL`, `busy_timeout=5000`,
  `foreign_keys=ON`.

### 3.8 Config (env vars) — `APP__*` overrides

Every field is defined on `Settings` in [`src/config.rs`](src/config.rs) and
has a default in `config/default.toml`. Prod must set:

- `APP__AUTH__ADMIN_TOKEN` — long random hex; required for bootstrap and
  emergency CLI.
- `APP__AUTH__CLERK_ISSUER` — Clerk issuer URL (e.g.
  `https://app.clerk.accounts.dev/...`). Required for user auth.
- `APP__AUTH__BOT_TOKEN_KEY` — 64-char hex (32 bytes). Required whenever
  `CLERK_ISSUER` is set; validated at boot ([`config.rs:108-119`](src/config.rs:108)).
- `APP__SERVER__CORS_ORIGINS` — semicolon-separated; include at least
  `https://lqcouncil.com`. If empty, CORS becomes permissive (dev only).
- `APP__MODELS__MINIMAX_API_KEY` and `APP__MODELS__OPUS_API_KEY` — needed to
  run debates.

### 3.9 Deploy / ops

- **systemd unit `bot-council.service`** exists on EVO under
  `/etc/systemd/system/bot-council.service`. Contents verified 2026-04-18:
  `Type=simple`, `User=james`, `WorkingDirectory=/home/james/bot-council`,
  `EnvironmentFile=/etc/bot-council.env`,
  `ExecStart=/home/james/bot-council/target/release/bot-council`,
  `Restart=on-failure`, `RestartSec=3`. Unit file is NOT in the repo. Move
  it to `deploy/bot-council.service` + checksum in INTEGRATIONS.md so
  drift is catchable.
- **Current runtime state is drift-prone.** At probe time, `systemctl is-active
  bot-council` returned `inactive` while a manual `./target/release/bot-council`
  was running as pid 2382046. Remediation: either always restart via
  `sudo systemctl restart bot-council` (CLAUDE.md's documented path) or
  kill the manual process and start the service. Do not leave both around
  — whichever was most recent wins :3100 and the other silently exits.
- **Env file:** `/etc/bot-council.env`, mode 0600, root:root. Read as root by
  systemd before dropping to `james`. Fields present at probe:
  `APP__AUTH__ADMIN_TOKEN`, `APP__AUTH__BOT_TOKEN_KEY`,
  `APP__AUTH__CLERK_ISSUER`, `APP__AUTH__CLERK_JWKS_URL`, `RUST_LOG`.
  Fields MISSING: `APP__MODELS__MINIMAX_API_KEY`, `APP__MODELS__OPUS_API_KEY`,
  `APP__SERVER__CORS_ORIGINS`. Any debate will fail until the model keys are
  added.
- **Deploy:** `./scripts/sync-evo.sh [build|check]` scp's `src/ tests/ config/
  migrations/ Cargo.*` to `~/bot-council/`, runs cargo on EVO, and
  (documented) `systemctl restart bot-council`.
- **Ingress (LQ Council):** Tailscale Funnel, not cloudflared. The Tailscale
  Funnel URL `https://james-nucbox-evo-x2.taila41c86.ts.net` maps `/` to
  `http://127.0.0.1:3100` (`tailscale funnel status`). A Vercel project
  `lqcouncil-api-proxy` rewrites `api.lqcouncil.com/*` onto that URL.
  Neither config is in the repo today.
- **Cloudflare tunnel `sovren-evo`** runs on the same EVO (`cloudflared run
  sovren-evo`, config at `~/.cloudflared/config.yml`). It belongs to the
  Sovren project; `council.sovren.xyz → :3100` is a route in that tunnel.
  Treat as external to LQ Council.

## 4. Frontend

### 4.1 Framework & build

- SvelteKit 2, Svelte 5 runes, TypeScript 5, Tailwind 4.
- `@sveltejs/adapter-static` with SPA fallback (`fallback: 'index.html'`)
  ([`frontend/svelte.config.js:8`](frontend/svelte.config.js:8)).
- `src/routes/+layout.ts` sets `prerender = false; ssr = false;` — fully
  client-rendered.
- Build output: static bundle under `frontend/build/`.
- `.npmrc` sets `legacy-peer-deps=true` to unblock Vercel installs.
- Node version NOT pinned (no `.nvmrc`); Vercel uses its default.

### 4.2 Vercel deploy

- `vercel.json` ([`frontend/vercel.json:2-5`](frontend/vercel.json:2)) has a
  single rule: rewrite `/(.*)` → `/index.html` for SPA routing.
- No redirects / headers / function config in-repo.
- Build settings live **only** in the Vercel dashboard:
  - Project is rooted at `frontend/`.
  - Production branch: `main`. Preview deploys on PRs.
  - Custom domain: `lqcouncil.com`.
  - Env vars set in Vercel (production scope):
    `PUBLIC_API_URL=https://api.lqcouncil.com`,
    `PUBLIC_CLERK_PUBLISHABLE_KEY=pk_live_…`.
  - Preview URLs follow Vercel's default pattern
    (`bot-council-git-<branch>-<team>.vercel.app`). Backend CORS must allow
    these or be permissive during testing.
- `.env.example` at `frontend/.env.example` is **stale** (points to the old
  Tailscale hostname). Not load-bearing because Vercel overrides it, but it
  misleads new contributors. Fix separately.

### 4.3 API client & SSE

- Base URL: `env.PUBLIC_API_URL` baked in at build
  ([`frontend/src/lib/api/client.ts:16`](frontend/src/lib/api/client.ts:16)).
- Every fetch ([`client.ts:27-52`](frontend/src/lib/api/client.ts:27)):
  - Grab Clerk session JWT via `getSessionToken()`
    ([`src/lib/auth/clerk.ts:27-32`](frontend/src/lib/auth/clerk.ts:27)).
  - Attach `Authorization: Bearer <jwt>`.
  - 10 s `AbortController` timeout (PR #27).
  - 401 → redirect to `/sign-in`.
  - Non-OK → throw `ApiError`.
- SSE URL builder: [`debateStreamUrl()` at `client.ts:115-118`](frontend/src/lib/api/client.ts:115)
  returns `${BASE_URL}/debates/${id}/stream?token=${encodeURIComponent(jwt)}`.
  Required because `EventSource` cannot set custom headers in the browser.

### 4.4 Auth flow (Clerk 6.7.2)

- Clerk singleton with a 12 s load timeout
  ([`src/lib/auth/clerk.ts:5-24`](frontend/src/lib/auth/clerk.ts:5)), added
  in PR #29 to stop indefinite loading spinners.
- Sign-in page mounts `clerk.mountSignIn(container, { fallbackRedirectUrl: '/',
  signUpFallbackRedirectUrl: '/' })` — both are Clerk v6's replacements for the
  deprecated `redirectUrl` option (fixed in PR #30).
- Root layout ([`src/routes/+layout.svelte`](frontend/src/routes/+layout.svelte))
  advances through named stages and surfaces the current stage + any error
  in the UI (PRs #31, #33, #34):
  `init → loading-clerk → checking-session → redirecting-sign-in →
  fetching-me → ready`. Missing `PUBLIC_CLERK_PUBLISHABLE_KEY` or
  `PUBLIC_API_URL` in the deployed bundle triggers an early fatal-error panel.

### 4.5 Routes

All under `frontend/src/routes/`.

| Path | Auth | Purpose |
|---|---|---|
| `/` | public | Landing + CTAs |
| `/sign-in` | public | Clerk-mounted sign-in form |
| `/how-it-works` | public | Protocol explanation |
| `/security` | public | Security documentation |
| `/debates` | auth | List debates |
| `/debates/new` | admin | Create debate |
| `/debates/[id]` | auth | Debate detail + live SSE transcript viewer (~575 lines) |
| `/bots` | auth | Bot list (different views for admin vs participant) |
| `/bots/submit` | auth | Submit a bot |
| `/bots/my-submissions` | auth | Own submissions |
| `/bots/criteria` | auth | Submission criteria — confidence `0-100` (fixed PR #32) |
| `/bots/guide` | auth | Integration guide — HTTPS endpoints (fixed PR #32) |
| `/admins` | admin | Admin roster management |
| `/settings` | admin | User settings |

## 5. Auth + data flow — end to end

Walk-through of a typical signed-in user loading `/debates/abc`:

1. Browser hits `https://lqcouncil.com/debates/abc` → Vercel CDN serves the
   prerendered shell with the SvelteKit bundle.
2. `+layout.svelte` boots Clerk. Stage `loading-clerk` is visible in the UI.
3. On Clerk ready, `+layout.svelte` calls `isSignedIn()`. If not signed in,
   redirects to `/sign-in`.
4. `refreshMe()` calls `GET https://api.lqcouncil.com/me` with the Clerk JWT.
   Backend runs `authenticate()` → JWKS verify → admin check. Returns
   `{ user_id, role: "admin" | "participant" }`.
5. The page `+page.svelte` opens `new EventSource(debateStreamUrl(id, jwt))`.
   The URL carries `?token=<jwt>` (EventSource can't send Authorization).
6. Backend's `/debates/{id}/stream` handler runs the same `authenticate()`,
   subscribes to the broadcast channel, streams SSE events as the tokio
   debate task emits them.
7. Cloudflare Tunnel keeps the TCP connection open; 30 s keepalive comments
   prevent idle timeout.
8. On `debate:completed` or `debate:failed`, the subscriber closes; the
   broadcast sender is dropped from `AppState` after a 60 s grace period.

## 6. Known gaps

- `deploy/bot-council.service` and the `cloudflared` config are NOT in the
  repo. Moving them under `deploy/` would make full-stack deploys
  reproducible from source.
- `frontend/.env.example` is stale; contributors cloning the repo will
  paste the wrong `PUBLIC_API_URL` unless corrected.
- No `.nvmrc` / `engines` field — Vercel and dev boxes can drift on Node
  version.
- No structured error endpoint on the backend (tracing is stdout-only); see
  the plan for the Clint integration, which proposes `/diag/errors`.
- No machine-readable bot validator endpoint; see the Clint plan proposing
  `/bots/validate` and `/bots/schema`.

## 7. How to keep this accurate

- When changing anything in §1–§5, update this file in the same PR.
- Cross-check `CLAUDE.md` against this file during every significant PR
  review. If they disagree, this file wins and CLAUDE.md gets fixed.
- On deploy changes (new host, new env var, new tunnel route), update §1
  and §3.8 before merging.
