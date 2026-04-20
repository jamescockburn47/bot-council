# INTEGRATIONS.md — operational playbook

How the LQ Bot Council plugs into every external system it depends on,
and how to keep those seams in repair. This is the "oh no it broke at
02:00" reference.

Scope: anything that isn't just source in this repo.

## 1. EVO X2 (primary host) + Tailscale

| Key | Value |
|---|---|
| Role | Runs `bot-council.service` (port 3100) and Clint (`clawdbot.service`, port 3000). |
| Tailscale address | `james@100.90.66.54` |
| LAN fallback | `james@10.0.0.2` (ethernet), `james@192.168.1.230` (WiFi) |
| User | `james` (NOT `pi`) |
| Project path (council) | `~/bot-council` |
| Project path (Clint) | `~/clawdbot` — deploy target; `~/clawdbot-claude-code` is a dev clone, not live |
| SSH key (from Windows) | `C:/Users/James/.ssh/id_ed25519` |
| Cargo env | `source ~/.cargo/env` in every SSH session |

Daily ops:

- `./scripts/sync-evo.sh` — scp src + tests + config + migrations + Cargo.*, run `cargo test`.
- `./scripts/sync-evo.sh check` — scp then `cargo check --tests` only.
- `./scripts/sync-evo.sh build` — `cargo build --release`.
- `./scripts/sync-evo.sh restart` — build + rewrite `SENTRY_RELEASE` in
  `/etc/bot-council.env` + `sudo systemctl restart bot-council`.

## 2. systemd units

### bot-council.service

- File: `/etc/systemd/system/bot-council.service` (this repo's `deploy/bot-council.service` is the canonical copy — diff both sides before edits).
- WorkingDirectory: `/home/james/bot-council`
- EnvironmentFile: `/etc/bot-council.env` (root:root 600 — systemd loads as root before dropping to User).
- Binary: `/home/james/bot-council/target/release/bot-council`
- `SENTRY_RELEASE` in the env file is rewritten by `sync-evo.sh restart` to the git SHA at deploy.

### clawdbot.service

- File: `/etc/systemd/system/clawdbot.service`
- WorkingDirectory: `/home/james/clawdbot`
- EnvironmentFile: `/home/james/clawdbot/.env` (mode 600)
- Binary: `/home/james/clawdbot/node_modules/.bin/tsx --env-file=.env src/index.js`
- Writes logs to journald; `sudo journalctl -u clawdbot -n 50` to tail.

## 3. Vercel

Two projects, both linked to the same GitHub repo but deployed under different aliases.

| Project | Path in repo | Domain | Purpose |
|---|---|---|---|
| `bot-council` | `frontend/` | `lqcouncil.com` (and `www.`) | Svelte static frontend. |
| `lqcouncil-api-proxy` | `lqcouncil-api-proxy/` | `api.lqcouncil.com` | Thin Next.js rewrite layer onto the Tailscale Funnel URL `https://james-nucbox-evo-x2.taila41c86.ts.net` → `127.0.0.1:3100` on EVO. |

Environment variables required on `bot-council` project:

- `PUBLIC_API_URL=https://api.lqcouncil.com`
- `PUBLIC_CLERK_PUBLISHABLE_KEY=<live Clerk pub key>`

Deploy:

```bash
vercel --cwd frontend deploy --prod --force --yes
```

**Build-cache gotcha:** if you only changed environment variables and didn't touch source, Vercel may pull a cached build. Trigger a fresh build with `--force` or by pushing any source file.

## 4. DNS + ingress (history — do not re-wire)

Before 2026-04-18 the proxy rewrote onto `council.sovren.xyz`, served by a Cloudflare tunnel `sovren-evo` with a `:3100` ingress. That tunnel has been **decommissioned**. If you reach a `council.sovren.xyz` reference in an older doc or comment, treat it as stale.

The only sanctioned public path is:

```
lqcouncil.com (Vercel: bot-council)
    → fetch('https://api.lqcouncil.com/...')
    → api.lqcouncil.com (Vercel: lqcouncil-api-proxy)
    → Tailscale Funnel URL (james-nucbox-evo-x2.taila41c86.ts.net)
    → 127.0.0.1:3100 on EVO (bot-council.service)
```

Clint on EVO skips this entirely and talks to `http://127.0.0.1:3100` via loopback (see `LQC_API_URL` in `clawdbot/.env`).

## 5. Clerk

| Key | Value |
|---|---|
| Issuer | `https://clerk.lqcouncil.com` |
| JWKS | `https://clerk.lqcouncil.com/.well-known/jwks.json` |
| Frontend publishable key | `PUBLIC_CLERK_PUBLISHABLE_KEY` on Vercel `bot-council` project |
| Backend issuer env var | `APP__AUTH__CLERK_ISSUER` in `/etc/bot-council.env` |
| Backend JWKS env var | `APP__AUTH__CLERK_JWKS_URL` (optional — defaults from issuer) |

Rotation: change both env vars together. Run `./scripts/check-auth-provider.sh` to verify the publishable-key instance ID matches the backend issuer (it greps the key's embedded domain and confirms JWKS returns 200).

## 6. Sentry

- **Backend** — Rust, DSN in `APP__SENTRY__DSN` on EVO at `/etc/bot-council.env`. Enriched in PR #49 with `release`, `debate_id`, `bot_id`, `user.id`, matched-path transactions.
- **Frontend** — SvelteKit, DSN in `PUBLIC_SENTRY_DSN`. Read at **build time** by Vite from `frontend/.env.production` (gitignored, lives on the build host alongside source). Originally a Vercel env var; migrated to a local file on 2026-04-20 after Vercel was retired in PR #62. Currently shares the backend's Sentry project DSN — split into a separate `bot-council-frontend` project later if the mixed stream gets noisy. Replay integration is **disabled** pending Cursor's DNS diagnosis (see `frontend/src/hooks.client.ts` comment).

Clint's Sentry integration (all optional — graceful no-op when unset):

- `LQC_SENTRY_API_TOKEN` — user auth token with `project:read` + `event:read`.
- `LQC_SENTRY_ORG` — slug.
- `LQC_SENTRY_PROJECT_BACKEND` — slug.
- `LQC_SENTRY_PROJECT_FRONTEND` — slug.
- `LQC_SENTRY_WEBHOOK_SECRET` — HMAC secret for the (currently unwired) webhook route.

Webhook route into Clint is **not yet live** — it requires changes to `src/http-server.js` which currently carries in-flight WIP. When ready, wire `POST /api/sentry-webhook` in Clint and configure Sentry to POST new-issue alerts there.

**Uptime monitors to add in Sentry UI** (one-time, 5-min interval):

- `https://api.lqcouncil.com/health`
- `https://clerk.lqcouncil.com/.well-known/jwks.json`

Route failures to the new-issue webhook when it lands; until then Sentry will email.

## 7. Anthropic

Two credentials:

| Key | Purpose | Env var |
|---|---|---|
| Standard API key | Synthesiser (Claude Opus) | `APP__MODELS__OPUS_API_KEY` |
| Admin API key (`sk-ant-admin-…`) | Organisation cost reports | Not yet set — needed for Phase 5 reconciliation |

Admin key is generated in the Claude Console → Organization → API Keys → "Create admin key". Never commits to git. Store in `/etc/bot-council.env` with mode 600 only when Phase 5 (cost tracking) ships.

## 8. MiniMax (M2.7)

| Key | Value |
|---|---|
| Base URL | `https://api.minimax.io/anthropic` (Anthropic-compatible endpoint) |
| API key env | `APP__MODELS__MINIMAX_API_KEY` |
| Model name | `APP__MODELS__MINIMAX_MODEL` (currently `MiniMax-M2.7`) |

No admin API — cost visibility is dashboard-only. For per-debate cost (Phase 5) we extract the `usage` block from each response.

## 9. Clint ↔ LQ Council seams

Set in `/home/james/clawdbot/.env`:

```bash
LQC_ENABLED=true
LQC_API_URL=http://127.0.0.1:3100
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

Tool visibility:

- `lqc_*` tools are stripped from every group chat that isn't `LQC_DEV_GROUP_JID`. DMs always allow them. The filter lives in `src/group-tool-policy.js` and reads `LQC_DEV_GROUP_JID` directly from `process.env` (not via the frozen config singleton — the test suite relies on that).

## 10. Rotation + backup

| Secret | Where stored | Rotation triggers |
|---|---|---|
| `APP__AUTH__BOT_TOKEN_KEY` (AES-256) | `/etc/bot-council.env` | Only if leak suspected — **rotating breaks all stored bot tokens** (ciphertext re-encrypt needed, not implemented). |
| `APP__AUTH__ADMIN_TOKEN` | `/etc/bot-council.env` | When pentest rotation policy requires; update Clint `LQC_ADMIN_TOKEN` in lock-step. |
| Clerk publishable + JWKS | Clerk dashboard + Vercel env + EVO env | Clerk rotation window (per their policy). |
| Anthropic API key | `/etc/bot-council.env` | On leak suspicion or 90-day policy. |
| MiniMax API key | `/etc/bot-council.env` | As per MiniMax policy. |

`council.db` backup: the SQLite file at `/home/james/bot-council/council.db` is the source of truth for bots + debates + responses. Back it up with `scripts/daily-backup.sh` (Clint side — adapt to also snapshot council.db nightly). **This is currently not scheduled** — add before going live with paying users.

## 11. Diagnostic endpoints (post 2026-04-18)

Added in PR #50. All `api.lqcouncil.com` paths:

- `GET /health` — public, returns 200.
- `GET /bots/schema` — public, JSON Schema derived from the Rust request/response types.
- `POST /bots/validate` — auth (any signed-in user), dry-run smoke test; no persistence.
- `GET /bots/{id}/history?limit=N` — auth, owner-gated unless admin.
- `GET /diag/health` — admin only, returns `{debates_in_flight, last_completion_ts, failure_rate_1h, release, …}`.

When something feels off, the first step is `curl https://api.lqcouncil.com/diag/health -H "Authorization: Bearer $ADMIN_TOKEN"`.
