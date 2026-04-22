# INTEGRATIONS.md — operational playbook

How the LQ Bot Council plugs into every external system it depends on, and how to keep those seams in repair. This is the "oh no it broke at 02:00" reference.

Scope: anything that isn't just source in this repo.

Current as of 2026-04-21. Keep in lockstep with `CLAUDE.md` and `ARCHITECTURE.md` — if they disagree, `ARCHITECTURE.md` wins.

## 1. EVO X2 (primary host) + Tailscale

| Key | Value |
|---|---|
| Role | Runs `bot-council.service` (port 3100), `sovren-cloudflared.service` (Cloudflare Tunnel), and Clint (`clawdbot.service`, port 3000). |
| Tailscale address | `james@100.90.66.54` |
| LAN fallback | `james@10.0.0.2` (ethernet), `james@192.168.1.230` (WiFi) |
| User | `james` (NOT `pi`) |
| Project path (council) | `~/bot-council` (scp'd source — **no git checkout on EVO**) |
| Project path (Clint) | `~/clawdbot` — deploy target; `~/clawdbot-claude-code` is a dev clone, not live |
| SSH key (from Windows) | `C:/Users/James/.ssh/id_ed25519` |
| Cargo env | `source ~/.cargo/env` in every SSH session |

Daily ops (see `CLAUDE.md` for the binding rules):

- **`./scripts/ship.sh`** — one-command deploy from laptop main branch. Preflight → frontend build → sync → rebuild on EVO → restart → health poll → public smoke. Writes `.last-known-good-sha` on EVO on success.
- **`./scripts/rollback.sh`** — binary-swap rollback (promotes `.prev` back to live). ~10 seconds. Never rebuilds.
- **`./scripts/sync-evo.sh {test,check,build,run}`** — dev iteration on EVO without a full deploy. `sync-evo.sh restart` is deprecated — use `ship.sh`.
- **CLI subcommands on EVO**:
  - `bot-council test-cleanup` — sweep test-flagged debates (systemd timer friendly).
  - `bot-council resynthesise [<id>] [--throttle-ms N]` — rebuild stored synthesis for one or all concluded debates. Launcher at `/home/james/resynth-launch.sh` sources env then invokes with `--throttle-ms 500`.

## 2. systemd units

### bot-council.service

- File: `/etc/systemd/system/bot-council.service` (canonical copy in repo at `deploy/bot-council.service` — diff both sides before edits).
- WorkingDirectory: `/home/james/bot-council`
- EnvironmentFile: `/etc/bot-council.env` (root:root 600 — systemd loads as root before dropping to `james`).
- Binary: `/home/james/bot-council/target/release/bot-council`
- Hardening (2026-04-20): `StartLimitBurst=5` / `StartLimitIntervalSec=300` in `[Unit]` caps restart loops; `ExecStartPre=/usr/bin/test -s /etc/bot-council.env` refuses empty env files; `TimeoutStartSec=120` caps startup hangs.
- `SENTRY_RELEASE` in the env file is rewritten by `ship.sh` (not `sync-evo.sh`) to the git SHA at deploy.
- Unit-file changes aren't covered by `ship.sh`. Install manually: `scp deploy/bot-council.service james@...:/tmp/ && ssh james@... "sudo cp /tmp/bot-council.service /etc/systemd/system/ && sudo systemctl daemon-reload && systemd-analyze verify /etc/systemd/system/bot-council.service && sudo systemctl restart bot-council"`.

### sovren-cloudflared.service

- Cloudflare Tunnel daemon — provides the public ingress for `lqcouncil.com`. Maintains 4× QUIC connections to London edges.
- Config: `~/.cloudflared/config.yml` (ingress rule: `lqcouncil.com → http://localhost:3100`).
- Creds: `~/.cloudflared/eef5ba90-6c24-4685-9c4d-e4d90e9f0db6.json` (tunnel UUID).
- Reference copies of both in `deploy/cloudflared/`.

### clawdbot.service

- File: `/etc/systemd/system/clawdbot.service`
- WorkingDirectory: `/home/james/clawdbot`
- EnvironmentFile: `/home/james/clawdbot/.env` (mode 600)
- Binary: `/home/james/clawdbot/node_modules/.bin/tsx --env-file=.env src/index.js`
- Writes logs to journald; `sudo journalctl -u clawdbot -n 50` to tail.

## 3. Ingress + DNS

Single-origin architecture. Vercel is fully retired (both the `bot-council` frontend project and `lqcouncil-api-proxy` removed on 2026-04-20).

```
Browser
  ↓ https://lqcouncil.com/...
Cloudflare edge (NS: gloria + mitch.ns.cloudflare.com; TLS, CDN, orange-cloud)
  ↓
Cloudflare Tunnel sovren-evo (UUID eef5ba90-6c24-4685-9c4d-e4d90e9f0db6)
  ↓ (4× QUIC to London edges)
cloudflared on EVO (systemd: sovren-cloudflared.service)
  ↓
Axum on :3100
  ├─ /api/*  → JSON API handlers
  ├─ /api/config.json → public runtime config (Clerk pk, Sentry env, release SHA)
  └─ /*      → tower-http ServeDir over ~/bot-council/frontend/build/, SPA fallback
```

**Retired hosts — do not wire new things to these:**

- `api.lqcouncil.com` — was the Vercel `lqcouncil-api-proxy` project (Next.js rewrite onto Tailscale Funnel). Vercel project removed; DNS not currently pointed anywhere.
- `council.sovren.xyz` — was a cloudflared ingress route pre-2026-04-18; decommissioned.
- Tailscale Funnel URL (`https://james-nucbox-evo-x2.taila41c86.ts.net`) — kept as a Tailscale-internal SSH/dev convenience, NOT in the public path.

Orphan DNS record: `lqcouncil.com.sovren.xyz` CNAME in the `sovren.xyz` zone (leftover from a `cloudflared tunnel route dns` run against the wrong zone auth). Harmless; delete when convenient.

## 4. Clerk

| Key | Value |
|---|---|
| Issuer | `https://clerk.lqcouncil.com` |
| JWKS | `https://clerk.lqcouncil.com/.well-known/jwks.json` |
| Frontend publishable key | `APP__AUTH__CLERK_PUBLISHABLE_KEY` in `/etc/bot-council.env` on EVO; surfaced to the browser at runtime via `GET /api/config.json` (no more `PUBLIC_CLERK_PUBLISHABLE_KEY` at build time) |
| Backend issuer env var | `APP__AUTH__CLERK_ISSUER` in `/etc/bot-council.env` |
| Backend JWKS env var | `APP__AUTH__CLERK_JWKS_URL` (optional — defaults from issuer) |

Boot hardening: JWKS startup fetch retries with exponential backoff (1s/2s/4s/8s/16s × ~91s worst case) then `bail!` — see `src/lib.rs`.

Rotation: change both env vars together, then `sudo systemctl restart bot-council`. Run `./scripts/check-auth-provider.sh` to verify the publishable-key instance ID matches the backend issuer (it greps the key's embedded domain and confirms JWKS returns 200).

## 5. Sentry

- **Backend** — Rust, DSN in `APP__SENTRY__DSN` on EVO at `/etc/bot-council.env`. Enriched in PR #49 with `release`, `debate_id`, `bot_id`, `user.id`, matched-path transactions.
- **Frontend** — SvelteKit, DSN in `PUBLIC_SENTRY_DSN`. Read at **build time** by Vite from `frontend/.env.production` (gitignored, lives on the build host / laptop). `frontend/.env.production` is a hard prerequisite for `ship.sh` — if missing, frontend Sentry silently no-ops (see CLAUDE.md "Build-host prerequisites"). Currently shares the backend's Sentry project DSN — split into a separate `bot-council-frontend` project later if the mixed stream gets noisy. Replay integration disabled pending Cursor's DNS diagnosis (see `frontend/src/hooks.client.ts` comment).

Clint's Sentry integration (all optional — graceful no-op when unset):

- `LQC_SENTRY_API_TOKEN` — user auth token with `project:read` + `event:read`.
- `LQC_SENTRY_ORG` — slug.
- `LQC_SENTRY_PROJECT_BACKEND` — slug.
- `LQC_SENTRY_PROJECT_FRONTEND` — slug.
- `LQC_SENTRY_WEBHOOK_SECRET` — HMAC secret for the (currently unwired) webhook route.

Webhook route into Clint is **not yet live** — it requires changes to `src/http-server.js` which currently carries in-flight WIP. When ready, wire `POST /api/sentry-webhook` in Clint and configure Sentry to POST new-issue alerts there.

**Uptime monitors to add in Sentry UI** (one-time, 5-min interval):

- `https://lqcouncil.com/api/health`
- `https://clerk.lqcouncil.com/.well-known/jwks.json`

Route failures to the new-issue webhook when it lands; until then Sentry will email.

## 6. MiniMax (M2.7) — live LLM provider

| Key | Value |
|---|---|
| Base URL | `https://api.minimax.io/v1/chat/completions` (OpenAI-compatible, Bearer auth) |
| API key env | `APP__MODELS__MINIMAX_API_KEY` in `/etc/bot-council.env` (populated) |
| Model name env | `APP__MODELS__MINIMAX_MODEL=MiniMax-M2.7` |
| Analyser route | `APP__MODELS__ANALYSIS_BASE_URL=https://api.minimax.io`, `APP__MODELS__ANALYSIS_MODEL=MiniMax-M2.7` |
| Synthesis route | `APP__MODELS__FINAL_SYNTHESIS_BASE_URL=https://api.minimax.io`, `APP__MODELS__FINAL_SYNTHESIS_MODEL=MiniMax-M2.7`, `APP__MODELS__FINAL_SYNTHESIS_WARMUP_ENABLED=false` |
| Extractor route | Reuses `analysis_base_url` + `analysis_model`. The `src/extractor/` pipeline calls `analyser::call_minimax` directly; no separate env var. |
| Plan | Pro-tier (high quota). Resynth batch safe at `--throttle-ms 500`. |

No admin API — cost visibility is dashboard-only. For per-debate cost (future Phase 5) we extract the `usage` block from each response.

**Three call paths now share the MiniMax endpoint** — analyser (per-round peer-score + challenge validation + pairing + divergence), final synthesis, and the structured-field extractor that turns text-only bot prose into `challenge`/`position_change` JSON in rounds 2 and 4. Extraction fires at most 5 bots × 2 rounds × 1 call = 10 extra MiniMax calls per debate when all participants are text-only; external bots short-circuit with no extractor call. Latency impact negligible on Pro tier.

**Rollback to local Gemma**: blank the three `APP__MODELS__*_BASE_URL` overrides, confirm `llama-server` is running on EVO `:8086` (`ps aux | grep llama-server`), `sudo systemctl restart bot-council`. Defaults in `config/default.toml` route back to the local model.

## 7. Anthropic (reserved, not live)

`APP__MODELS__OPUS_API_KEY` exists in the env file but is **empty**. It was populated earlier in the project then zeroed on 2026-04-20 (stray key installed by an earlier session without user awareness). Reserved for a future Claude Opus synthesis path — not currently on any code path.

Admin key (`sk-ant-admin-…`) is needed for organisation cost reports; generated in the Claude Console → Organization → API Keys → "Create admin key". Not yet set; needed only if Phase 5 cost reconciliation ships.

## 8. Clint ↔ LQ Council seams

Set in `/home/james/clawdbot/.env`:

```bash
LQC_ENABLED=true
LQC_API_URL=http://127.0.0.1:3100   # loopback, not lqcouncil.com
LQC_ADMIN_TOKEN=<same value as APP__AUTH__ADMIN_TOKEN in /etc/bot-council.env>
LQC_DEV_GROUP_JID=<the dev WA group JID — empty until confirmed>
# Optional Sentry wiring:
LQC_SENTRY_API_TOKEN=
LQC_SENTRY_ORG=
LQC_SENTRY_PROJECT_BACKEND=
LQC_SENTRY_PROJECT_FRONTEND=
LQC_SENTRY_WEBHOOK_SECRET=
# Optional digest routing:
LQC_DIGEST_GROUP_JID=
LQC_NUDGE_FAILURE_THRESHOLD=0.7
```

Clint on EVO skips the Cloudflare Tunnel entirely and talks to `http://127.0.0.1:3100` over loopback.

Tool visibility: `lqc_*` tools are stripped from every group chat that isn't `LQC_DEV_GROUP_JID`. DMs always allow them. The filter lives in `src/group-tool-policy.js` and reads `LQC_DEV_GROUP_JID` directly from `process.env` (not via the frozen config singleton — the test suite relies on that).

**Text-only bot mode drift gap (2026-04-22):** `lqc_validate_bot` and `lqc_dry_run_debate` in the clawdbot repo still speak the legacy `/debate` contract. They will reject or mis-diagnose any bot registered with `bot_kind: "text_only"`. Until those tools are rewired, operators onboarding text-only bots must use the `/bots/submit` web flow; the Clint-driven validation path is external-mode only. Clint's curated knowledge file (`data/lqcouncil-knowledge.json`) regenerates nightly from `BOT_AUTHORING.md` in this repo, so Clint's *answers* about text-only mode will be current even while the *tool handlers* remain legacy. Fix is scoped for a dedicated session in the `~/clawdbot` repo.

## 9. Rotation + backup

| Secret | Where stored | Rotation triggers |
|---|---|---|
| `APP__AUTH__BOT_TOKEN_KEY` (AES-256) | `/etc/bot-council.env` | Only if leak suspected — **rotating breaks all stored bot tokens** (ciphertext re-encrypt not implemented). |
| `APP__AUTH__ADMIN_TOKEN` | `/etc/bot-council.env` | When pentest rotation policy requires; update Clint `LQC_ADMIN_TOKEN` in lock-step. |
| `APP__AUTH__CLERK_PUBLISHABLE_KEY` + issuer/JWKS | Clerk dashboard + EVO env | Clerk rotation window (per their policy). Frontend picks it up automatically via `/api/config.json`. |
| `APP__MODELS__MINIMAX_API_KEY` | `/etc/bot-council.env` | On leak suspicion or MiniMax policy. |
| `APP__MODELS__OPUS_API_KEY` (when wired) | `/etc/bot-council.env` | On leak suspicion or Anthropic policy. |
| `APP__SENTRY__DSN` | `/etc/bot-council.env` + `frontend/.env.production` | Sentry project re-provisioning. |

`council.db` backup: the SQLite file at `/home/james/bot-council/data/council.db` is the source of truth for bots + debates + responses. **Not currently backed up on a schedule** — add before going live with paying users. A nightly snapshot to a second location would be the simplest lift.

## 10. Diagnostic endpoints

All paths under `https://lqcouncil.com`:

- `GET /api/health` — public, returns `{"status":"ok"}`. Also available as `GET /health` (no prefix) and `GET /api/diag/health`.
- `GET /api/config.json` — public, returns Clerk pk, Sentry env, release SHA.
- `GET /api/diag/models` — **admin only**, returns the effective model routing (analyser + final-synthesis URLs/models). First thing to check if "did the env overrides land?".
- `GET /api/bots/schema` — auth, JSON Schema derived from the Rust request/response types.
- `POST /api/bots/validate` — auth, dry-run smoke test; no persistence.
- `GET /api/bots/{id}/history?limit=N` — auth, owner-gated unless admin.
- `GET /api/bots/{id}/analytics` — auth, per-bot performance metrics.
- `GET /api/debates/{id}/transcript` — auth. For text-only bots, each `TranscriptEntry` carries `extraction_metadata` (`{challenge?: {source, quote}, position_change?: {source, quote}}`) so the frontend can render "extracted" badges and the verbatim source quote inline. External bots emit structured fields directly; `extraction_metadata` is `null` for their rows.

First step when something feels off: `curl https://lqcouncil.com/api/diag/models -H "Authorization: Bearer $ADMIN_TOKEN"` to confirm the model routing matches expectation.
