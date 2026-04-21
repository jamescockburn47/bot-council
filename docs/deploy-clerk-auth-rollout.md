# Deploy Runbook — Clerk Auth Rollout (Plan 1) — HISTORICAL

> **HISTORICAL DOCUMENT.** This runbook describes the one-time cutover from
> pre-Clerk dev-mode auto-admin to the current Clerk-JWT + admin-registry +
> AES-encrypted-bot-token state. The cutover completed in April 2026 (see
> PR history around that time). The Step 6 "Frontend deploy (Vercel)"
> instructions are superseded — Vercel was fully retired 2026-04-20 and
> the frontend is now served by Axum on EVO. For current deploy procedure
> use `./scripts/ship.sh`; for current env surface see `ARCHITECTURE.md §3.9`
> and `CLAUDE.md`. Kept as a reference for how the cutover was executed.

This runbook covers cutting the EVO over from the pre-Clerk dev-mode
auto-admin state to Clerk JWT + in-app admin registry + AES-encrypted bot
tokens. Everything it depends on is already on `main` — the deploy has not
yet been performed.

**What's already merged to `main`:**

| PR | Subject |
|---|---|
| [#20](https://github.com/jamescockburn47/bot-council/pull/20) | Plan 1: Clerk RS256 JWKS verification, RequireAuth/RequireAdmin, AES-256-GCM bot tokens, submission feedback, handler collapse |
| [#21](https://github.com/jamescockburn47/bot-council/pull/21) | In-app admin registry — `admins` table + `/admins` page, runtime promote/demote, no preset user_ids |
| [#22](https://github.com/jamescockburn47/bot-council/pull/22) | SSE `?token=` query-param auth fallback (EventSource cannot set headers) |
| [#23](https://github.com/jamescockburn47/bot-council/pull/23) | CLAUDE.md ops details + `scripts/sync-evo.sh` |
| [#24](https://github.com/jamescockburn47/bot-council/pull/24) | `BotTokenKey` newtype with `ZeroizeOnDrop` |
| [#25](https://github.com/jamescockburn47/bot-council/pull/25) | Drop legacy `token_hash` + `active` columns |

52 backend tests + frontend build green on `main`.

---

## Pre-flight checklist (do this before the deploy window)

### 1. Generate a bot token encryption key

```bash
openssl rand -hex 32
```

Save the 64-character hex output. This is `APP__AUTH__BOT_TOKEN_KEY`. **Never rotate
without re-encrypting all bot rows** — rotating this key breaks every existing bot.

### 2. Generate (or reuse) the admin bearer token

```bash
openssl rand -hex 32
```

Save the output. This is `APP__AUTH__ADMIN_TOKEN`. It grants full admin access via
`Authorization: Bearer <token>` — used for CLI ops, emergency access if Clerk is
down, and bootstrapping the first in-app admin via `POST /admins`.

### 3. Confirm existing bot state on EVO

```bash
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 \
  "sqlite3 ~/bot-council/data/council.db 'SELECT id, name, status FROM bots;'"
```

Any pre-rollout bots (most likely candidate: Clint, James's admin bot) have their
bearer tokens stored as the old `token_hash` — migration `20260416000003` drops
that column, so those bots will have no usable token after the restart and their
smoke tests / debate calls will fail. Re-submit each one via `/bots/submit` (Step 9
below). Takes ~30 seconds per bot.

---

## Deploy window

### Step 1 — Push the source to EVO

From any up-to-date checkout of `main`:

```bash
./scripts/sync-evo.sh check    # sanity — cargo check --tests on EVO
```

The script scp's `src tests config migrations Cargo.toml Cargo.lock` to
`james@100.90.66.54:~/bot-council/` and runs the chosen cargo command there.

### Step 2 — Apply the new migrations

Migrations run automatically on next startup via `sqlx::migrate!`. No manual step
required. Three migrations will run on the existing prod DB:

1. `20260416000001_bot_submission_cleanup.sql` — adds `token_ciphertext`, `rejection_reason`, `idx_bots_status_reviewable`.
2. `20260416000002_admin_registry.sql` — adds the `admins` and `seen_users` tables.
3. `20260416000003_drop_legacy_bot_columns.sql` — drops `token_hash`, `active`.

If you want to pre-apply them (e.g. to inspect the schema before the restart):

```bash
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 \
  "cd ~/bot-council && for f in migrations/202604160000*.sql; do \
     echo '---' \$f; sqlite3 data/council.db <\$f; done"
```

### Step 3 — Set environment variables on the EVO

Edit the systemd unit's environment file (or wherever you currently set config).

```
APP__AUTH__ADMIN_TOKEN=<your-admin-token>
APP__AUTH__CLERK_ISSUER=https://<your-clerk-instance>.clerk.accounts.dev
APP__AUTH__BOT_TOKEN_KEY=<your-64-char-hex-key>
```

No preset admin list required. Admins are managed at runtime via the `admins`
table and the `/admins` page (see Step 7).

### Step 4 — Build and restart

```bash
./scripts/sync-evo.sh restart   # sync + cargo build --release + sudo systemctl restart bot-council
```

If the config is malformed, the service will refuse to start and the error will be
in `journalctl -u bot-council`. Fix the config and retry.

### Step 5 — Smoke-test the API

```bash
# /health should return 200 without auth
curl -sI https://lqcouncil.com/health | head -1

# /me without auth should return 401
curl -sI https://lqcouncil.com/me | head -1

# /me with the admin bearer should return 200 + admin role
curl -s -H "Authorization: Bearer <APP__AUTH__ADMIN_TOKEN>" https://lqcouncil.com/me

# POST /debates without auth should return 401
curl -sI -X POST https://lqcouncil.com/debates \
  -H "content-type: application/json" -d '{"topic":"x"}' | head -1
```

### Step 6 — Frontend deploy (Vercel)

Vercel auto-deploys `main` on every push, so the frontend for this release is
already built and hosted. You only need to intervene if the Clerk publishable key
hasn't been set on the Vercel project:

1. Vercel dashboard → bot-council → Settings → Environment Variables.
2. Ensure `PUBLIC_CLERK_PUBLISHABLE_KEY=pk_live_<real-key>` is set on **Production**
   (and Preview, if you want preview deploys to work too).
3. Ensure `PUBLIC_API_URL=https://lqcouncil.com`.
4. If any were changed, trigger a redeploy from the Vercel dashboard.

### Step 7 — Bootstrap the first admin

No admins exist in the DB yet. The bootstrap flow uses the static admin bearer
token to promote the first Clerk user into the `admins` table.

1. Open https://lqcouncil.com in a browser → redirected to `/sign-in`.
2. Sign in with James's Clerk account.
3. You'll land on `/` as a **member** — expected; `admins` is still empty.
4. Retrieve your Clerk `user_id` (format `user_2…`). Either from `/me`:
   ```bash
   curl -s -H "Authorization: Bearer $CLERK_SESSION_JWT" https://lqcouncil.com/me
   ```
   or from the Clerk dashboard.
5. Promote yourself using the admin bearer token (one time only):
   ```bash
   curl -X POST https://lqcouncil.com/admins \
     -H "Authorization: Bearer $APP__AUTH__ADMIN_TOKEN" \
     -H "content-type: application/json" \
     -d '{"user_id":"user_2YOUR_ID_HERE"}'
   ```
6. Refresh the browser. You should now see the admin UI (New Debate button,
   Admins nav entry, etc.).
7. Navigate to `/admins`. Promote the other 4 admins (Jamie, Artur, Ray, YC) by
   clicking "Promote" next to each name. They must have signed in at least once
   to appear in the Signed-in users table — ping them to sign in first if they
   haven't.

### Step 8 — Sign-in smoke test (browser)

With yourself as admin:

1. Submit a dummy bot via `/bots/submit` — as admin, it lands in `status=active`.
2. Verify it appears in `/bots` under Active.

Sign out, sign in as a non-admin test Clerk user:

1. Sidebar shows "Signed in as member".
2. No Admins nav entry; no New Debate button on `/debates`.
3. Navigating directly to `/debates/new` or `/admins` redirects away.
4. Submitting a bot via `/bots/submit` creates it with `status=pending`.
5. `/bots/my-submissions` shows the pending bot.

### Step 9 — Resubmit existing bots

If the Step 3 DB check found any pre-rollout bots, resubmit them now through
`/bots/submit`. For Clint:

```
Name:          Clint
Endpoint URL:  <Clint's current /debate URL>
Token:         <value of DASHBOARD_TOKEN from EVO .env>
Model family:  minimax
Description:   LQ Council's own EVO-hosted bot.
```

Plan 2 (spec §§18–19) will enforce the MiniMax participant constraint; until
then, `model_family` is informational.

---

## Rollback

This rollout is three PRs' worth of commits on `main` — rollback is squash-commit
granularity, not per-file. Safe rollback order:

```bash
# Revert the smallest / last-landed things first if the issue is narrow.
# Full rollback of the auth/RBAC surface reverts #20 — do this last.
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 \
  "cd ~/bot-council && git fetch && \
   git revert --no-edit <merge-sha> && \
   cargo build --release && sudo systemctl restart bot-council"
```

Merge SHAs (latest first): `fab2430` (#25), `a826fbe` (#24), `6183fa6` (#22),
`b126770` (#21), `b471aa1` (#20).

Migrations are additive (new columns, new tables). #25 is the only destructive
one — it drops `token_hash` and `active`. A rollback past #25 would need to
re-add those columns manually if the restored code still reads them. The live
code on `main` after this rollout does not, so a revert of #25 alone has no
effect on the runtime path.

---

## Known deferrals

- `/bots/schema` validator endpoint and Plan 2 response-normaliser
  consolidation — see spec §§18–19 in
  `docs/superpowers/specs/2026-04-16-clerk-auth-and-bot-submission-cleanup-design.md`.
- MiniMax participant-model constraint — spec §19.
- Per-vendor JSON-mode guidance rewrite in `/bots/guide` — spec §18.2.
- Admin UI "repair-rate" column for bot quality monitoring — spec §18.3.

`BotTokenKey` zeroisation (#24) and the `token_hash` / `active` column drops
(#25) landed before this deploy; they are not outstanding.
