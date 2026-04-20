# Vercel Inventory — captured 2026-04-20 16:15 BST

Snapshot taken before the Cloudflare + single-origin EVO migration (Phases E/F of the hardening plan). Purpose: everything needed to reproduce production without Vercel.

## Vercel account

- User: `jamescockburn47`
- Team: `james-cockburns-projects`

## Projects to retire

### 1. `bot-council` (frontend)

| Field | Value |
|---|---|
| Project ID | `prj_YiU0VrfuP7rsCMllA3rzOOGt8SAd` |
| Production URL | `https://lqcouncil.com` |
| Root directory | `frontend` |
| Output directory | `build` |
| Node.js | 24.x |
| Framework | "Other" (SvelteKit + adapter-static) |
| Created | 2026-04-16 08:35 BST |

Environment variables (production):

| Key | Value (location) |
|---|---|
| `PUBLIC_API_URL` | `https://api.lqcouncil.com` — **eliminated in Phase C** (frontend uses relative `/api/*`) |
| `PUBLIC_CLERK_PUBLISHABLE_KEY` | `pk_live_<REDACTED>` — **moves to EVO `/etc/bot-council.env`** as `APP__AUTH__CLERK_PUBLISHABLE_KEY` and gets served at runtime via `/config.json` |
| `PUBLIC_SENTRY_ENVIRONMENT` | `prod` — **moves to EVO `/etc/bot-council.env`** as a frontend-facing field on `/config.json` |

Actual values preserved locally (gitignored): `frontend/.env.production.vercel-snapshot`.

Vercel auto-provisioned values (no action needed): `VERCEL`, `VERCEL_ENV`, `VERCEL_URL`, `VERCEL_GIT_*`, `VERCEL_OIDC_TOKEN`, `NX_DAEMON`, `TURBO_*`.

### 2. `lqcouncil-api-proxy` (reverse proxy)

| Field | Value |
|---|---|
| Project ID | `prj_KpG5QTCHwrR8T59mxVvM9ws7D6KA` |
| Production URL | `https://lqcouncil-api-proxy.vercel.app` |
| Domain | `api.lqcouncil.com` |
| Root directory | `.` (the project's linked dir, not the repo's `deploy/vercel-api-proxy/` which is just the config source) |
| Framework | Next.js |
| Env vars | none |
| Created | 2026-04-17 09:25 BST |

Routing (from [deploy/vercel-api-proxy/vercel.json](deploy/vercel-api-proxy/vercel.json)):
```json
{
  "rewrites": [
    { "source": "/(.*)", "destination": "https://james-nucbox-evo-x2.taila41c86.ts.net/$1" }
  ]
}
```

**Replaced in Phase E** by a single Cloudflare DNS record (`lqcouncil.com` → `CNAME james-nucbox-evo-x2.taila41c86.ts.net`, proxied) which also covers the frontend path. `api.lqcouncil.com` subdomain becomes unnecessary once the frontend serves relative `/api/*`.

## Domains

| Domain | Registrar | Current nameservers | Expires |
|---|---|---|---|
| `lqcouncil.com` | **Vercel** | Vercel | 2027-04-16 |

Registration stays at Vercel. Only nameservers change in Phase E1 — the domain registration doesn't need to be transferred. In the Vercel dashboard: Domains → `lqcouncil.com` → Nameservers tab → change to Cloudflare's assigned pair.

## Cloudflare replacement plan (Phase E)

1. User creates a Cloudflare free-tier account, adds `lqcouncil.com` as a site. Cloudflare assigns two nameservers.
2. User changes nameservers at Vercel to Cloudflare's. Propagation 5–60 min.
3. Once Cloudflare marks the zone active, add DNS records:
   - `lqcouncil.com` `A/AAAA` → proxied CNAME flattening → `james-nucbox-evo-x2.taila41c86.ts.net` (orange cloud)
   - (Optional) `www.lqcouncil.com` → redirect to apex
4. Cache rules: cache `/_app/*` + hashed assets for 1 year; bypass cache for `/api/*`, `/config.json`, `/_app/version.json`.
5. SSL/TLS mode: Full (strict). Tailscale Funnel provides a real cert.
6. Enable "Always Online" for graceful degradation during EVO downtime.

## Decommission plan (Phase F, after Cloudflare stable for 24h)

```bash
vercel remove bot-council --yes
vercel remove lqcouncil-api-proxy --yes
```

Also delete from repo:
- `frontend/vercel.json`
- `deploy/vercel-api-proxy/` directory
- `.vercel/` and `.proxy-vercel-link/` local dirs (already gitignored)

Keep the domain at Vercel as registrar. No cost implication since they include DNS hosting free; we just stop using it.

## Config migration mapping

| Vercel env var | New home |
|---|---|
| `PUBLIC_API_URL` | **Eliminated.** Frontend will call relative `/api/*` URLs after Phase C. |
| `PUBLIC_CLERK_PUBLISHABLE_KEY` | `/etc/bot-council.env` on EVO as `APP__AUTH__CLERK_PUBLISHABLE_KEY`, served at runtime from new `GET /config.json` endpoint. |
| `PUBLIC_SENTRY_ENVIRONMENT` | Already in EVO env file as `APP__SENTRY__ENVIRONMENT`; will also be added to `/config.json` response so frontend Sentry init gets the tag. |

## Other Vercel projects on this account (not affected)

Left alone; listed here so the migration doesn't accidentally touch them:
`nostalgic-hawking-45bba2`, `tarf`, `taste-trawler`, `sovren-simple`, `frontend`, `sovren`, `tinywriters`, `guru-bot`, `lquorum`, `gamesincjr`, `sovren-public`, `lawandcode`, `jamescockburn-io`, `artyartbox`, `act-*`.

## What's no longer needed after Phase F

- Vercel CLI auth (user can keep logged in for other projects)
- `.vercel/` dirs in `frontend/` and `.proxy-vercel-link/`
- The `vercel` npm dep if any (check `frontend/package.json` — not currently present)
- Any GitHub Actions / webhooks pointing at Vercel (none exist per `gh workflow list`)
