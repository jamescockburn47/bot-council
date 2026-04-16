# Deploy Runbook — Clerk Auth Rollout (Plan 1)

This document walks through deploying commits `de61b3d..4e2b14c` to the EVO X2 and
cutting over authentication from the dev-mode auto-admin fallback to Clerk + admin
user allowlist. Follow the steps in order.

**Branch:** `claude/reverent-goldwasser` (to be merged to `main` after verification)
**Release build verified on EVO:** Finished `release` profile [optimized] target(s) in 8.31s
**Test suite:** 21 tests pass (unit + integration + 6 config validation tests)

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
`Authorization: Bearer <token>` — use for CLI ops and emergency access if Clerk is
down.

### 3. Confirm existing bot state on EVO

```bash
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 \
  "sqlite3 ~/bot-council/data/council.db 'SELECT id, name, status, submitted_by FROM bots;'"
```

If any bots are currently `active` in the DB they will have `token_ciphertext IS NULL`
and their smoke tests / debate calls will fail after this rollout. Every such bot
must be re-submitted by its owner (or by you, using their bearer token). Most likely
candidate: Clint (James's admin bot). Re-submission takes 30 seconds via `/bots/submit`.

---

## Deploy window

### Step 1 — Push the source to EVO

```bash
cd "C:/Users/James/Desktop/LQ projects/Bot council/.claude/worktrees/reverent-goldwasser"
scp -i C:/Users/James/.ssh/id_ed25519 -r \
  src tests config migrations Cargo.toml Cargo.lock \
  james@100.90.66.54:~/bot-council/
```

(The subagent workflow did this after most tasks, so this step may be a no-op.)

### Step 2 — Apply the new migration

The migration runs automatically on next startup via `sqlx::migrate!`. No manual step
needed, but you can pre-apply it with:

```bash
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 \
  "sqlite3 ~/bot-council/data/council.db <~/bot-council/migrations/20260416000001_bot_submission_cleanup.sql"
```

### Step 3 — Set environment variables on the EVO

Edit the systemd unit's environment file (or wherever you currently set config).

```
APP__AUTH__ADMIN_TOKEN=<your-admin-token>
APP__AUTH__CLERK_ISSUER=https://<your-clerk-instance>.clerk.accounts.dev
APP__AUTH__BOT_TOKEN_KEY=<your-64-char-hex-key>
```

No preset admin list needed. Admins are managed at runtime via the `admins` table
and the `/admins` page (see Step 7).

### Step 4 — Build and restart

```bash
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 \
  "source ~/.cargo/env && cd ~/bot-council && cargo build --release && sudo systemctl restart bot-council"
```

If the config is malformed, the service will refuse to start and the error will be in
`journalctl -u bot-council`. Fix the config and retry.

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

### Step 6 — Deploy the frontend

```bash
cd "C:/Users/James/Desktop/LQ projects/Bot council/.claude/worktrees/reverent-goldwasser/frontend"

# Add the real Clerk publishable key to the frontend's .env.production (or
# whatever Vercel uses)
cat > .env.production <<EOF
PUBLIC_API_URL=https://lqcouncil.com
PUBLIC_CLERK_PUBLISHABLE_KEY=pk_live_<your-real-key>
EOF

npm run build
# Deploy the build/ directory however you currently deploy the frontend
# (Vercel, Cloudflare Pages, etc.)
```

### Step 7 — Bootstrap the first admin

No admins exist yet in the DB. The bootstrap flow uses the static admin bearer token.

1. Open https://lqcouncil.com in a browser → redirected to `/sign-in`.
2. Sign up / sign in with your (James's) Clerk account.
3. You'll land on `/` as a **member** — this is expected, no admin rows exist.
4. Hit `/me` in your browser devtools or:
   ```bash
   curl -s -H "Authorization: Bearer $CLERK_SESSION_JWT" https://lqcouncil.com/me
   ```
   Record your Clerk user_id from the response (format `user_2abc...`). You can also
   read it from the Clerk dashboard.
5. Promote yourself using the admin bearer token (one time only):
   ```bash
   curl -X POST https://lqcouncil.com/admins \
     -H "Authorization: Bearer $APP__AUTH__ADMIN_TOKEN" \
     -H "content-type: application/json" \
     -d '{"user_id":"user_2YOUR_ID_HERE"}'
   ```
6. Refresh the browser. You should now see the admin UI (New Debate button, Admins
   nav entry, etc.).
7. Navigate to `/admins`. Promote the other 4 admins (Jamie, Artur, Ray, YC) by
   clicking "Promote" next to each name. They must have signed in at least once to
   appear in the Signed-in users table — ping them to sign in first if they haven't.

### Step 8 — Sign-in smoke test (browser)

With yourself as admin:

1. Submit a dummy bot via `/bots/submit` — as admin, it lands in `status=active`.
2. Verify in `/bots` Active tab.

Sign out, sign in as a non-admin test Clerk user:

1. **Sidebar shows "Signed in as member"**.
2. **No Admins nav entry**, **no New Debate button** on `/debates`.
3. Navigating directly to `/debates/new` or `/admins` redirects away.
4. Submitting a bot via `/bots/submit` creates it with `status=pending`.
5. `/bots/my-submissions` shows the pending bot.

### Step 9 — Resubmit existing bots

If the pre-flight DB check found any `active` bots with null `token_ciphertext`,
resubmit them now through `/bots/submit`. For Clint:

```
Name:          Clint
Endpoint URL:  <Clint's current /debate URL>
Token:         <value of DASHBOARD_TOKEN from EVO .env>
Model family:  minimax
Description:   LQ Council's own EVO-hosted bot.
```

Task 10 / Plan 2 will enforce the MiniMax participant constraint — for now, model
family is informational.

### Step 10 — Tag and merge

Once sign-in + submit + approve flow works end-to-end:

```bash
cd "C:/Users/James/Desktop/LQ projects/Bot council/.claude/worktrees/reverent-goldwasser"
git tag plan1-clerk-rollout
git push origin claude/reverent-goldwasser --tags
gh pr create --base main --head claude/reverent-goldwasser \
  --title "Plan 1: Clerk auth, RBAC, encrypted tokens, submission feedback" \
  --body-file docs/deploy-clerk-auth-rollout.md
```

---

## Rollback

Each of the 13 implementation commits is independently revertible. If a bug surfaces
after step 4 (backend restart) but before step 6 (frontend deploy), the backend alone
can be reverted with:

```bash
ssh -i C:/Users/James/.ssh/id_ed25519 james@100.90.66.54 \
  "cd ~/bot-council && git revert bb95481..4e2b14c && cargo build --release && sudo systemctl restart bot-council"
```

The migration is additive (new columns only; legacy `token_hash` retained) so DB
state survives a rollback without data loss.

---

## What's in this rollout

Summary of the 13 commits landed on `claude/reverent-goldwasser`:

| Commit | Subject |
|---|---|
| `de61b3d` | AES-256-GCM crypto module |
| `84251dd` | DB migration: token_ciphertext + rejection_reason |
| `de8bce0` | AuthConfig: admin_user_ids + bot_token_key |
| `6c74036` | JWKS cache with hot-swap + background refresh |
| `3028e0c` | RS256 JWT verification + RequireAuth/RequireAdmin |
| `ca5b27f` | Route wiring + POST /debates admin-only |
| `17aaf02` | transition_bot_status + reject reason + smoke_test_failed |
| `f480acb` | Smoke-test error classifier |
| `bb95481` | Encrypt tokens on submit, decrypt on outbound |
| `6464c7f` | Frontend Clerk integration |
| `1c2d184` | Submission feedback banners + reject modal |
| `c1d309e` | Hide new-debate controls from participants |
| `4e2b14c` | Boot-time config validation + remove deprecated aliases |

---

## Known deferrals

- **`token_hash` column** still exists on `bots` with an empty-string placeholder for
  new rows. Dropped in a follow-up migration after one release.
- **`active` column** still exists in the DB; the API no longer exposes it. Dropped
  in the same follow-up migration.
- **`BotTokenKey` zeroisation.** Raw `[u8; 32]` key lives in process memory without
  `zeroize::Zeroizing`. Consider adding if the deployment threat model shifts (e.g.
  multi-tenant, untrusted sidecar processes).
- **`#[cfg(test)]` participant impersonation hook.** Not implemented; participant
  path is verified manually in Step 7. Add if automated participant tests become
  necessary.
- **Plan 2** — bot author UX: response normaliser consolidation, `/bots/schema`
  validator endpoint, MiniMax participant constraint, guide rewrites. Separate spec
  section §§18–19. Not started yet.
