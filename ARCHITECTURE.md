# ARCHITECTURE.md — LQ Bot Council

Forensic description of how LQ Council is built, served, and interacts
end-to-end. Keep this file and `CLAUDE.md` in lockstep: when one changes,
update the other in the same branch.

## 0. Regression prevention contract

Primary historical regression cause: frontend and backend validated/deployed from different branches/worktrees, and source-on-EVO silently drifting from git. Those two failure modes are now structurally prevented by three mechanisms:

1. **Single-origin architecture** — Axum on EVO serves both the static SvelteKit bundle (`/*`) and the JSON API (`/api/*`). One binary, one deploy, one SHA. The frontend doesn't exist as a separate deploy target; you can't deploy them out of sync because there's only one deploy target.
2. **`scripts/ship.sh`** — single command that refuses to ship from a dirty tree or non-main branch, runs the frontend build, syncs to EVO, rebuilds, restarts, and health-polls. Non-zero exit at the failing stage. Writes `.last-known-good-sha` on EVO on success so `scripts/rollback.sh` has a target.
3. **GitHub Actions CI** — `cargo fmt/clippy/test` + `svelte-check + vite build` gate every PR. Enable branch protection (Settings → Branches → `main`) to make the checks blocking for merges.

Required release contract: `ship.sh` is green end-to-end (all 7 stages) and `curl https://lqcouncil.com/api/health` returns JSON `{"status":"ok"}`. No other manual checks required.

## 1. Deployment topology

Current as of 2026-04-21 (Vercel fully retired; Cloudflare Tunnel in front; LLM routing on MiniMax-M2.7).

```
 Browser
   │
   │  https://lqcouncil.com/...
   ▼
 Cloudflare edge (NS: gloria + mitch.ns.cloudflare.com)
   (CDN, TLS cert for lqcouncil.com, apex CNAME flattened to CF anycast,
    orange-cloud proxied; optional WAF / Bot Fight Mode / Always Online)
   │
   ▼
 Cloudflare Tunnel (tunnel: "sovren-evo",
                    UUID eef5ba90-6c24-4685-9c4d-e4d90e9f0db6,
                    4× QUIC connections to London edges lhr01/14/18/20)
   │
   ▼
 cloudflared on EVO (systemd: sovren-cloudflared.service, active+enabled)
   (ingress rules in ~/.cloudflared/config.yml; single route:
    lqcouncil.com → http://localhost:3100)
   │
   ▼
 bot-council on EVO (systemd: bot-council.service, active+enabled)
   axum on 0.0.0.0:3100
   ├─ /api/*            API handlers (see §3.2)
   ├─ /api/config.json  public runtime config (Clerk pk_*, sentry env, release SHA)
   └─ /*                tower-http ServeDir(~/bot-council/frontend/build/)
                        with index.html SPA fallback
   SQLite: ~/bot-council/data/council.db (WAL)
   env:    /etc/bot-council.env (root:root 0600, sourced by systemd unit)
   LLM:    MiniMax-M2.7 at https://api.minimax.io (OpenAI-compatible, Bearer auth,
           configured via APP__MODELS__{ANALYSIS,FINAL_SYNTHESIS,MINIMAX}_BASE_URL
           env overrides; MINIMAX_API_KEY also in env)
   Rollback LLM: llama-server from llama.cpp at http://localhost:8086
                 (gemma-4-31B-it-Q4_K_M.gguf). Not currently on the call path.
                 Unset the APP__MODELS__*_BASE_URL overrides and restart
                 bot-council to route back to local.
```

Response headers confirm the path: `Server: cloudflare`, `cf-cache-status: DYNAMIC`, `CF-RAY: <id>-LHR`, HTTP/2.

**Ingress history (for future-me).**
- Through 2026-04-17: cloudflared tunnel (sovren-evo) with a `:3100` ingress on `council.sovren.xyz`. Reachable via `api.lqcouncil.com` via a Vercel proxy rewrite to that Sovren hostname.
- 2026-04-18: Vercel proxy re-pointed to Tailscale Funnel (`james-nucbox-evo-x2.taila41c86.ts.net`); cloudflared tunnel disabled.
- 2026-04-20: Full Cloudflare cutover. Vercel bot-council frontend project and lqcouncil-api-proxy project both removed. `lqcouncil.com` now served directly by the same `sovren-evo` tunnel (which was re-enabled and reconfigured for the apex of `lqcouncil.com`). Tailscale Funnel no longer in the public path; kept as a Tailscale-internal SSH/dev convenience.

**Probed 2026-04-20 18:15 BST (final Cloudflare cutover verification):**
- `GET https://lqcouncil.com/` → 200 `text/html` (SvelteKit SPA shell served by Axum ServeDir)
- `GET https://lqcouncil.com/api/health` → 200 `{"status":"ok"}`, Server: cloudflare
- `GET https://lqcouncil.com/api/config.json` → 200 with pk_live_* Clerk key, `api_base=/api`, current release SHA
- `GET http://127.0.0.1:3100/api/health` on EVO → 200
- `systemctl is-active bot-council` → active
- `systemctl is-active sovren-cloudflared` → active

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
│   │   └── extraction.rs          post-round structured-field extraction (text-only bots)
│   ├── extractor/                 MiniMax-backed extractor for challenge/position_change
│   │                              (prompt builder, serde types, quote-substring verifier)
│   ├── bot_client/                HTTP client to bot endpoints
│   │   ├── mod.rs                 DebateRoundRequest/Response types, dispatch_round_request
│   │   ├── text_only.rs           POST {prompt,session_id} → {text} dispatch
│   │   └── position_scoring.rs    legacy pre-Phase-1 position + scoring dispatch
│   └── config.rs                  Settings struct + validation
├── frontend/                      SvelteKit SPA (details §4)
├── reference/                     author-facing reference hooks
│   ├── text-only-hook/            Python + Node snippets for the default contract
│   └── debate-endpoint-*.{js,py}  legacy /debate reference implementations
│   ├── svelte.config.js           adapter-static, SPA fallback
│   ├── src/lib/auth/clerk.ts      Clerk singleton, 12 s timeout (PR #29)
│   ├── src/lib/api/client.ts      fetch wrapper + SSE URL builder
│   └── src/routes/                pages (see §4.4)
├── scripts/ship.sh                single-command deploy (see §3.9)
├── scripts/rollback.sh            binary-swap rollback via .prev
├── scripts/sync-evo.sh            scp + cargo + systemctl helper for
│                                  dev iteration (test/check/build/run)
├── deploy/bot-council.service     systemd unit (see §3.9)
├── deploy/cloudflared/            reference copies of tunnel config + creds
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
| `RequireAuth` | `GET /me`, `GET /bots`, `POST /bots`, `GET /bots/my-submissions`, `GET /debates`, `GET /debates/{id}`, `GET /debates/{id}/transcript`, `GET /debates/{id}/synthesis`, `GET /debates/{id}/stream` |
| `RequireAdmin` | `PATCH /bots/{id}/{approve\|reject\|deactivate\|reactivate}`, `PATCH /bots/{id}/test`, `POST /debates`, `PATCH /debates/{id}/archive`, `DELETE /debates/{id}`, `GET/POST /admins`, `DELETE /admins/{user_id}`, `GET /users`, `GET /diag/models` |

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
   issuer check and 30 s leeway → extract `sub` as Clerk `user_id` and optional
   email claims → best-effort upsert into `seen_users` (never breaks auth) →
   check `admins` table → return `AuthIdentity::Admin{…}` or `::Participant{user_id}`.

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

- `POST /bots` validates name, endpoint URL (`https://` in release builds;
  `http://localhost|127.0.0.1` also allowed in debug), and token; encrypts
  the token with AES-256-GCM using `BotTokenKey`; accepts an optional
  `bot_kind` (`"external" | "text_only"`, default `external`); status =
  `active` for admins, `pending` for others.
- Token ciphertext layout: `nonce(12) || ciphertext || tag(16)` as a BLOB
  ([`src/api/bot_token_crypto.rs:66-76`](src/api/bot_token_crypto.rs:66)).
- `PATCH /bots/{id}/approve` runs `smoke_test_bot(capture_introduction=true)`.
  Flow differs by `bot_kind`:
  - **Text-only bots** — first a single introduction probe: POST
    `{session_id: "smoke-introduction", prompt: "Introduce yourself in
    two or three sentences — who you are, what you bring to a debate, what
    makes you distinct from a generic assistant."}`. The non-empty `text`
    is persisted to `bots.introduction` via `queries::set_bot_introduction`
    BEFORE the status transition (so a DB failure doesn't leave a live bot
    without its introduction). Then five hook-shape round probes, each
    validated only for non-empty `text`.
  - **External bots** — no introduction probe. Five probes use the legacy
    `DebateRoundRequest` body and are validated against the round-specific
    schema in `validate_smoke_json_for_round`.
- `PATCH /bots/{id}/test` and the `POST /debates` preflight call
  `smoke_test_bot(capture_introduction=false)` so the intro probe fires
  only at approval time — never on every debate creation.
- Errors are run through `classify_smoke_test_error()` to produce a
  human-readable `rejection_reason` (DNS, connection, TLS, 401/403, HTTP
  status, JSON/missing field, empty text for text-only).
- State machine: `pending → active | smoke_test_failed | rejected`;
  `active ↔ inactive`.
- Dispatch at runtime goes through `bot_client::dispatch_round_request`
  which branches on `bot_kind`: `external` → `send_debate_request` (full
  contract), `text_only` → `send_text_only_request` (returns
  `DebateRoundResponse` with only the `response` field populated; all
  structured fields come back `None` and get populated by the extractor
  post-round if needed).
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
- **Post-round extraction** (rounds 2 and 4): after bot responses are
  collected but before the analyser runs, each response from a
  `text_only` bot whose required structured field is `None` flows through
  `orchestrator::extraction::extract_if_needed(models, bot_kind, target,
  &mut response)`. The extractor builds a constrained prompt (forbidding
  inference, requiring verbatim source quotes, fencing bot text as data
  against prompt injection), calls MiniMax, parses the `RawExtraction`
  schema, verifies every field's declared quote against the bot's raw
  text via `extractor::verify::quote_is_substring_of` (whitespace-
  normalised, case-sensitive substring), and if every check passes
  populates the typed field on the response. Any failure — MiniMax
  unreachable, unparseable body, fabricated quote, `serde_json::from_value`
  shape mismatch — downgrades provenance to `source: "extraction_failed"`
  with `quote: None`. Provenance is persisted as
  `responses.extraction_metadata = {"challenge": {source, quote}, ...}`
  and surfaced to the frontend via the transcript endpoint. External
  bots short-circuit at `bot_kind != "text_only"` (no extraction call).
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
  8. `20260418000004_seen_user_identity_metadata.sql` — adds `seen_users.email` and `seen_users.display_name`.
  9. `20260419000001_responses_error_detail.sql` — adds `responses.error_detail` for post-mortem error capture.
  10. `20260421000001_debate_archived_at.sql` — adds nullable `debates.archived_at` for soft archival (Phase D3).
  11. `20260422000001_text_only_bot_mode.sql` — adds `bots.bot_kind TEXT NOT NULL DEFAULT 'external'`, nullable `bots.introduction`, and nullable `responses.extraction_metadata` for the text-only bot mode.
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
- `APP__MODELS__MINIMAX_API_KEY` — MiniMax Bearer token. Required while live
  routing is on MiniMax-M2.7 (see §3.9). `APP__MODELS__OPUS_API_KEY` is
  reserved for a future Opus synthesis path and is currently blank.
- `APP__MODELS__{ANALYSIS,FINAL_SYNTHESIS,MINIMAX}_BASE_URL` +
  `APP__MODELS__{ANALYSIS,FINAL_SYNTHESIS,MINIMAX}_MODEL` — override the
  local-llama defaults in `config/default.toml`. Currently all three point
  at `https://api.minimax.io` + `MiniMax-M2.7`. Unsetting them (or clearing
  them in the env file) routes the service back to the local llama-server
  at `:8086`.
- `APP__MODELS__FINAL_SYNTHESIS_WARMUP_ENABLED` — set to `false` when the
  final-synthesis route is a hosted API (no warmup to do); currently false.

### 3.9 Deploy / ops

- **systemd unit `bot-council.service`** — IN REPO at [`deploy/bot-council.service`](deploy/bot-council.service). Hardened 2026-04-20:
  - `[Unit] StartLimitIntervalSec=300, StartLimitBurst=5` — bounds restart loops; 5 failed starts in 300s puts the unit in a terminal failed state (must be in `[Unit]` not `[Service]` per systemd docs).
  - `[Service] ExecStartPre=/usr/bin/test -s /etc/bot-council.env` — refuses to start on empty env file.
  - `[Service] TimeoutStartSec=120` — caps startup hangs; leaves 30s headroom over the JWKS backoff worst-case.
  - `Restart=on-failure`, `RestartSec=3`, `User=james`, `Group=james`, `WorkingDirectory=/home/james/bot-council`, `EnvironmentFile=/etc/bot-council.env`, `ExecStart=/home/james/bot-council/target/release/bot-council`.
- **Installing unit changes** (not covered by `ship.sh`):
  ```
  scp deploy/bot-council.service james@...:/tmp/
  ssh james@... "sudo cp /tmp/bot-council.service /etc/systemd/system/ && sudo systemctl daemon-reload && systemd-analyze verify /etc/systemd/system/bot-council.service && sudo systemctl restart bot-council"
  ```
  Ignore any `systemd-analyze` warnings about unrelated units — it scans all units on the box.
- **Env file:** `/etc/bot-council.env`, mode 0600, root:root. Read as root by systemd before dropping to `james`. Current keys (as of 2026-04-21):
  - Auth: `APP__AUTH__ADMIN_TOKEN`, `APP__AUTH__BOT_TOKEN_KEY`, `APP__AUTH__CLERK_ISSUER`, `APP__AUTH__CLERK_JWKS_URL`, `APP__AUTH__CLERK_PUBLISHABLE_KEY`.
  - LLM routing (live on MiniMax): `APP__MODELS__MINIMAX_API_KEY` (Bearer token, populated), `APP__MODELS__MINIMAX_BASE_URL=https://api.minimax.io`, `APP__MODELS__MINIMAX_MODEL=MiniMax-M2.7`, `APP__MODELS__ANALYSIS_BASE_URL=https://api.minimax.io`, `APP__MODELS__ANALYSIS_MODEL=MiniMax-M2.7`, `APP__MODELS__FINAL_SYNTHESIS_BASE_URL=https://api.minimax.io`, `APP__MODELS__FINAL_SYNTHESIS_MODEL=MiniMax-M2.7`, `APP__MODELS__FINAL_SYNTHESIS_WARMUP_ENABLED=false`. `APP__MODELS__OPUS_API_KEY` is present but empty.
  - Observability: `APP__SENTRY__DSN`, `APP__SENTRY__ENVIRONMENT`, `SENTRY_RELEASE` (git SHA, auto-written by `ship.sh`).
- **Deploy:** `./scripts/ship.sh` from the laptop — see §0 and `CLAUDE.md`. Rollback via `./scripts/rollback.sh` (binary-swap). For iteration on EVO without a full deploy, `./scripts/sync-evo.sh {test,check,build,run}` still works.
- **Ingress:** Cloudflare Tunnel (`sovren-evo`, systemd: `sovren-cloudflared.service`). Ingress rule in `~/.cloudflared/config.yml`: `lqcouncil.com → http://localhost:3100`. Tunnel creds at `~/.cloudflared/eef5ba90-6c24-4685-9c4d-e4d90e9f0db6.json`. Reference copies of both in [`deploy/cloudflared/`](deploy/cloudflared/). See §1 for full chain.
- **LLM — live path:** MiniMax-M2.7 hosted API at `https://api.minimax.io/v1/chat/completions`. All analyser + final-synthesis calls emit `Authorization: Bearer $MINIMAX_API_KEY`. Quota + latency characteristics live at the provider.
- **LLM — rollback path:** `llama-server` from llama.cpp on EVO `:8086` serving `gemma-4-31B-it-Q4_K_M.gguf`. Managed outside this repo. Not currently on the call path. To roll back: remove (or blank) the three `APP__MODELS__*_BASE_URL` env overrides, confirm llama-server is running (`systemctl status` for whatever unit owns it, or `ps aux | grep llama-server`), restart bot-council.
- **CLI subcommands:** the `bot-council` binary accepts:
  - `bot-council` (default) — run the HTTP service.
  - `bot-council test-cleanup` — delete debates flagged as test runs, cascading through child tables. Designed for a systemd timer (one-shot).
  - `bot-council resynthesise [<debate_id>] [--throttle-ms N]` — rebuild the stored synthesis for one or all concluded, non-archived debates by re-running the analyser + synthesiser against the existing transcript. `--throttle-ms` (default 2000) sleeps between debates to respect provider rate limits. Use after a synthesis-prompt change; see CLAUDE.md operational lesson #16.

## 4. Frontend

### 4.1 Framework & build

- SvelteKit 2, Svelte 5 runes, TypeScript 5, Tailwind 4.
- `@sveltejs/adapter-static` with SPA fallback (`fallback: 'index.html'`)
  ([`frontend/svelte.config.js:8`](frontend/svelte.config.js:8)).
- `src/routes/+layout.ts` sets `prerender = false; ssr = false;` — fully
  client-rendered.
- Build output: static bundle under `frontend/build/`. `ship.sh` runs `npm ci && npm run build` on the laptop and scp's `frontend/build/` to `~/bot-council/frontend/build/` on EVO; there is no frontend build step on EVO.
- `.npmrc` keeps `legacy-peer-deps=true` to accept npm's pre-7 peer-dep resolution (Clerk + SvelteKit alignment); this applies to local installs as well, not just the retired Vercel build.
- Node version not pinned in the repo. `ship.sh` runs against whatever the laptop has; CI pins to `actions/setup-node@v4` with its default (currently 22.x). Pin with a `.nvmrc` if drift becomes an issue.

### 4.2 Frontend serving (Vercel retired 2026-04-20)

- **Build output is part of the deploy**: `ship.sh` runs `npm run build` locally, scp's `frontend/build/` to `~/bot-council/frontend/build/` on EVO alongside the Rust source, and Axum's `tower-http::ServeDir` serves it from `/*`.
- **SPA fallback** via `ServeDir::new(...).not_found_service(ServeFile::new(<dir>/index.html))` in [src/lib.rs](src/lib.rs) — any path that doesn't match a real file returns `index.html`, letting SvelteKit's client-side router take over.
- **Build gate** in [frontend/package.json](frontend/package.json): `npm run build` chains `svelte-kit sync` + `svelte-check` + `vite build` — type errors block the build.
- **Dev flow**: `npm run dev` inside `frontend/` proxies `/api/*` to `http://127.0.0.1:3100` via [frontend/vite.config.ts](frontend/vite.config.ts); override with `VITE_BACKEND_URL`.
- **No more `PUBLIC_API_URL` / `PUBLIC_CLERK_PUBLISHABLE_KEY` at build time.** Runtime config is fetched by the frontend from `GET /api/config.json` on first load (served by [src/api/config_json.rs](src/api/config_json.rs) from the backend's own config). Replaces Vercel build-time env injection.
- **`.env.example`** in [frontend/.env.example](frontend/.env.example) documents that no env vars are needed for prod; optional `VITE_BACKEND_URL` for dev only.

### 4.3 API client & SSE

- **Base URL is relative.** Same-origin; the client uses `/api/...` paths directly and relies on Axum serving both `/*` and `/api/*`. No `PUBLIC_API_URL` is baked in at build. (On `npm run dev`, Vite's proxy in `frontend/vite.config.ts` forwards `/api/*` to `http://127.0.0.1:3100`; override with `VITE_BACKEND_URL`.)
- **Runtime config** comes from `GET /api/config.json` on first load — supplies the Clerk publishable key, Sentry environment, and release SHA. Replaces the old Vercel build-time env injection.
- Every fetch ([`client.ts`](frontend/src/lib/api/client.ts)):
  - Grab Clerk session JWT via `getSessionToken()` ([`src/lib/auth/clerk.ts`](frontend/src/lib/auth/clerk.ts)).
  - Attach `Authorization: Bearer <jwt>`.
  - `AbortController` timeout (PR #27).
  - 401 → redirect to `/sign-in`.
  - Non-OK → throw `ApiError`.
- SSE URL builder: `debateStreamUrl()` in [`client.ts`](frontend/src/lib/api/client.ts) returns `/api/debates/${id}/stream?token=${encodeURIComponent(jwt)}`. Required because `EventSource` cannot set custom headers in the browser. Backend accepts the token via `?token=` as an alternative to the `Authorization` header.

### 4.4 Auth flow (Clerk 6.7.2)

- Clerk singleton with a 12 s load timeout
  ([`src/lib/auth/clerk.ts:5-24`](frontend/src/lib/auth/clerk.ts:5)), added
  in PR #29 to stop indefinite loading spinners.
- Sign-in page uses hosted Clerk redirect flow (`clerk.redirectToSignIn(...)`)
  and sends successful sign-in to app-home (`/debates`).
- Root layout ([`src/routes/+layout.svelte`](frontend/src/routes/+layout.svelte))
  uses explicit route policy flags (`mustBeSignedIn`, `rendersWithoutSession`,
  `signedInRedirectTo`) and stage-based bootstrap:
  `init → loading-clerk → checking-session → redirecting-sign-in |
  redirecting-signed-in → fetching-me → ready`.

### 4.5 Routes

All under `frontend/src/routes/`.

| Path | Auth | Purpose |
|---|---|---|
| `/` | public | Landing + CTAs; signed-in users redirect to `/debates` |
| `/sign-in` | public | Hosted Clerk redirect shell |
| `/how-it-works` | public | Protocol explanation |
| `/security` | public | Security documentation |
| `/debates` | auth | List debates |
| `/debates/new` | admin | Create debate |
| `/debates/[id]` | auth | Debate detail + live SSE transcript viewer (~575 lines) |
| `/bots` | auth | Bot list (different views for admin vs participant) |
| `/bots/submit` | auth | Submit a bot |
| `/bots/my-submissions` | auth | Own submissions |
| `/bots/criteria` | auth | Approval criteria — leads with introduction-as-primary-signal and coherence across rounds |
| `/bots/guide` | auth | Integration guide — text-only contract (`{prompt,session_id}` → `{text}`), super-prompt for Claude Code, Python + Node snippets |
| `/admins` | admin | Admin roster management |
| `/settings` | admin | User settings |

## 5. Auth + data flow — end to end

Walk-through of a typical signed-in user loading `/debates/abc`:

1. Browser hits `https://lqcouncil.com/debates/abc` → Cloudflare edge → Cloudflare Tunnel → Axum on EVO. Axum's `ServeDir` resolves to `index.html` (SPA fallback — no file at `/debates/abc` on disk).
2. Browser loads the SvelteKit bundle; before Clerk init, fetches `GET /api/config.json` to discover the `publishable_key`.
3. `+layout.svelte` boots Clerk with that key. Stage `loading-clerk` is visible in the UI.
4. On Clerk ready, `+layout.svelte` calls `isSignedIn()`. If not signed in, redirects to `/sign-in`.
5. `refreshMe()` calls `GET /api/me` (relative URL) with the Clerk JWT. Backend runs `authenticate()` → JWKS verify → admin check. Returns `{ user_id, role: "admin" | "participant" }`.
6. The page `+page.svelte` opens `new EventSource(debateStreamUrl(id, jwt))`, which resolves to `/api/debates/{id}/stream?token=<jwt>` (EventSource can't send Authorization headers).
7. Backend's handler runs the same `authenticate()`, subscribes to the debate's broadcast channel, streams SSE events as the tokio debate task emits them.
8. Cloudflare Tunnel keeps the QUIC connection open; 30s keepalive comments prevent idle timeout.
9. On `debate:completed` or `debate:failed`, the subscriber closes; the broadcast sender is dropped from `AppState` after a 60s grace period.

## 6. Known gaps

- `frontend/.env.example` still mentions old Vercel wording; minor.
- One orphan Cloudflare DNS record: `lqcouncil.com.sovren.xyz` CNAME in the `sovren.xyz` zone (left over from a `cloudflared tunnel route dns` command that ran with `sovren.xyz`-scoped auth). Harmless; delete when convenient.
- No `.nvmrc` / `engines` field — local dev + CI can drift on Node version.
- No structured error endpoint on the backend beyond Sentry (tracing is stdout-only); see the plan for the Clint integration, which proposes `/diag/errors`.
- The existing `/bots/schema` + `/bots/validate` routes are mounted (see §3.2) but not yet documented end-to-end.
- `config/default.toml` still advertises the local llama-server as the default for `analysis_*` + `final_synthesis_*`. Truthful as a rollback target, but a new reader could easily miss that production is env-overridden to MiniMax. See CLAUDE.md operational lesson #14.

## 6.1 Branch/worktree discipline

- Start every task from `origin/main`, never from `master` or stale feature branches.
- Run branch preflight before creating new branch:
  - `./scripts/branch-preflight.ps1`
- Use clean worktrees for parallel tasks.
- Merge and delete task branches promptly to prevent divergence buildup.
- Run periodic stale-branch cleanup:
  - `./scripts/branch-cleanup.ps1` (dry-run)
  - `./scripts/branch-cleanup.ps1 -Apply` (delete gone-upstream branches not attached to worktrees)

## 7. How to keep this accurate

- When changing anything in §1–§5, update this file in the same PR.
- Cross-check `CLAUDE.md` against this file during every significant PR
  review. If they disagree, this file wins and CLAUDE.md gets fixed.
- On deploy changes (new host, new env var, new tunnel route), update §1
  and §3.8 before merging.
