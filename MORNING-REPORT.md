# Morning report — Clint ↔ LQ Council integration

> **HISTORICAL SNAPSHOT (2026-04-19).** Point-in-time status report, not a living
> document. Several statements are stale — notably `api.lqcouncil.com` (Vercel
> proxy, retired 2026-04-20) no longer exists; Vercel is fully retired; live
> LLM routing is now MiniMax-M2.7. For current state see `CLAUDE.md`
> ("Current state" section), `ARCHITECTURE.md`, and `INTEGRATIONS.md`.

Completed overnight, 2026-04-18 → 2026-04-19. Everything below is live in prod unless explicitly flagged "deferred" or "needs your hand".

## What shipped

Eight PRs across two repos, all merged and deployed.

### LQ Council backend (`jamescockburn47/bot-council`)

| PR | Title | Phase |
|----|-------|-------|
| [#48](https://github.com/jamescockburn47/bot-council/pull/48) | chore: enforce branch hygiene and unified release gate | Phase -1 |
| [#49](https://github.com/jamescockburn47/bot-council/pull/49) | feat(obs): enrich Sentry events with debate_id, user, matched path, release | Phase 0 |
| [#50](https://github.com/jamescockburn47/bot-council/pull/50) | feat(api): diagnostic endpoints + response error classification | Phase 1 |
| [#51](https://github.com/jamescockburn47/bot-council/pull/51) | docs(ops): add INTEGRATIONS.md operational playbook | Phase 6 |

Deploy status: `api.lqcouncil.com/health` returns 200, `bot-council.service` active, release tag now matches the deployed git SHA.

### Clint (`jamescockburn47/clawd-admin`)

| PR | Title | Phase |
|----|-------|-------|
| [#1](https://github.com/jamescockburn47/clawd-admin/pull/1) | feat(lqcouncil): read-only tools for the Bot Council integration | Phase 2 |
| [#2](https://github.com/jamescockburn47/clawd-admin/pull/2) | feat(prompt): LQ Council knowledge fragment for dev group | Phase 3 |
| [#3](https://github.com/jamescockburn47/clawd-admin/pull/3) | feat(lqcouncil): proactive monitoring + why-failed / recent-errors tools | Phase 4 |
| [#4](https://github.com/jamescockburn47/clawd-admin/pull/4) | feat(lqcouncil): weekly digest + daily bot-failure nudge | Phase 4c |

Deploy status: `clawdbot.service` active. All Phase 2–4c code synced to `~/clawdbot/` on EVO via scp (git pull from EVO couldn't auth to GitHub — see "follow-ups" below).

## Tools Clint now has (gated to `LQC_DEV_GROUP_JID` + owner DMs)

Read-only:
- `lqc_status` — harness health + recent debates
- `lqc_list_debates` — paginated list with optional status filter
- `lqc_debate_detail` — topic, bots, roles, rankings
- `lqc_list_bots` — registered bots with status
- `lqc_bot_schema` — live-derived JSON Schema (Draft 2020-12)
- `lqc_validate_bot` — dry-run smoke test against a candidate endpoint
- `lqc_bot_diagnose` — aggregate per-bot `error_kind` patterns + per-kind remediations
- `lqc_bot_author_guide` — onboarding explainer (topics: overview, schema, rounds, failure_modes, testing, all)
- `lqc_onboarding_checklist` — admission checklist with state inferred from the harness
- `lqc_self_describe` — list of the above
- `lqc_why_failed` — transcript + Sentry correlation for a specific debate
- `lqc_recent_errors` — Sentry REST query, graceful when not configured

Proactive signals (scheduler, 60s tick):
- Debate transitions to `failed` → dev-group alert (edge-triggered).
- Debate non-terminal > 30 min → "stuck" alert (15-min cooldown).
- 1h failure rate > 25 % → dev-group alert (15-min cooldown).
- Bot's last 20 rounds show a new dominant `error_kind` ≥ 40 % of rounds → "pattern shift" alert.

Scheduled:
- Sunday 09:00 London — weekly digest (debate counts, top bots by success, Sentry top issues if configured).
- Daily 10:00 London — failure nudge for any active bot with > 70 % failure rate.

## What needs your hand in the morning

**1. Set the dev-group JID so alerts actually post.**

In `~/clawdbot/.env` on EVO, set:

```
LQC_DEV_GROUP_JID=<the LQ dev WA group JID>
```

Then `sudo systemctl restart clawdbot`. Until this is set, `lqc_*` tools only appear in owner DMs, and proactive alerts log but don't send.

**2. (Optional) Wire Sentry API access so `lqc_recent_errors` + `lqc_why_failed` can correlate upstream.**

Generate a Sentry user auth token with `project:read` + `event:read`, then add to `~/clawdbot/.env`:

```
LQC_SENTRY_API_TOKEN=<token>
LQC_SENTRY_ORG=<org slug>
LQC_SENTRY_PROJECT_BACKEND=<backend slug>
LQC_SENTRY_PROJECT_FRONTEND=<frontend slug>
```

All tools degrade gracefully without these — they just skip the Sentry block.

**3. Verify in WhatsApp (five tests, 5 minutes):**

- Owner DM: `@clawd lqc_self_describe` → expect the 10-tool list.
- Owner DM: `@clawd lqc_status` → expect health output with release SHA.
- Owner DM: `@clawd lqc_bot_schema` → expect JSON Schema properties for `DebateRoundRequest`/`Response`.
- Non-dev group (once dev JID is set): `@clawd what's the LQ Council status?` → Clint should NOT have the tool; expect a decline or generic answer.
- Dev group (once JID is set): same question → Clint should use `lqc_status` and reply with fresh data.

**4. Sentry UI (two 2-minute jobs):**

- Add uptime monitor on `https://api.lqcouncil.com/health` (5-min interval).
- Add uptime monitor on `https://clerk.lqcouncil.com/.well-known/jwks.json` (5-min interval).

## Backend verification (run these before you trust the deploy)

```bash
# Public
curl -sS https://api.lqcouncil.com/health                          # expect 200 + "OK"
curl -sS https://api.lqcouncil.com/bots/schema | jq .dialect       # expect the 2020-12 draft URL

# Admin
ADMIN=af78eb543e9fa563096c2a004c37c53deae1bb1899a493e1a5d9d707716ec0a6
curl -sS -H "Authorization: Bearer $ADMIN" https://api.lqcouncil.com/diag/health | jq
# expect JSON with debates_in_flight, failure_rate_1h, release = <sha>
```

## What I deliberately did NOT do (and why)

- **Phase 5 (cost tracking + model switching).** Biggest piece of the plan. Skipped because it needs a new migration (`debate_usage`), a new pricing module, a wrap around the MiniMax/Anthropic callers, a budget-enforcement hook in the orchestrator, four new endpoints, and the Anthropic Admin API key (which only you can generate). Running that unattended overnight without your verification would have violated your "evidence before code, pause between phases" rule.
- **Phase 7 (write-path admin actions: approve/reject/promote/start-debate/restart-service).** Your plan required an explicit "yes do Phase 7" before starting. I didn't get one.
- **Phase 4b (`lqc_rerun_smoke_tests`).** Needs a new `POST /admin/rerun-smoke-tests` backend endpoint plus a `last_smoke_test_at`/`last_smoke_test_result` migration. Ship in a follow-up — not overnight without your review. `lqc_dry_run_debate` is covered by `lqc_validate_bot` already.
- **SSE subscriber in Clint.** The plan called for it. Would need `eventsource` npm package (not installed) or manual SSE parsing, plus state management. Polling at 60s gives near-real-time alerts with dramatically less code. If you want the sub-second latency, I can add a follow-up — design is simple.
- **Sentry webhook route (`POST /api/sentry-webhook` on Clint).** Requires editing `src/http-server.js`, which already carries your in-flight `/debate` endpoint WIP. I'd have conflicted with it. The webhook handler itself (`src/lqcouncil/sentry-webhook.js`) isn't written either — deferred until your WIP lands so the route registration lives alongside `/debate`.
- **Sentry frontend Replay integration re-enable.** Waiting on Cursor's DNS fix per your plan.

## Preserved WIP on Clint (NOT touched)

You had uncommitted work on Clint when I started:

- `src/http-server.js` (modified) — adds a `/debate` endpoint.
- `src/debate-handler.js` (untracked, 354 lines) — handler for the above, using MiniMax + web_search/memory_search/web_fetch tools.
- `docs/superpowers/specs/2026-04-12-overnight-simplification-design.md` (untracked).

None of these are in my commits. They remain in the working tree on the `main` branch of both `~/clawdbot-claude-code` (local) and `~/clawdbot` (EVO) exactly as you left them.

## Follow-ups I'd queue

**Small (<1 hour each):**
- Round 1-4 orchestrator error-kind classification. Only round 0 currently records `error_kind` on abstention (see `src/orchestrator/rounds/round0.rs`); rounds 1-4 follow the identical pattern but weren't ported to keep the PR scope tight. Mechanical copy-paste of the round0 refactor.
- Re-enable git-based deploy for Clint. Currently `~/clawdbot` on EVO uses HTTPS remote without cached creds, so `git pull` fails. Either switch `git remote set-url origin git@github.com:jamescockburn47/clawd-admin.git` with an SSH key on EVO, or install a PAT via `git credential-manager`. Today I used scp which works but isn't the documented workflow.
- Pre-existing test failure: `test/agency.test.js` imports `DEFAULT_AMBIENT_AGENCY_POLICY` from `src/agency/policy.js` which doesn't exist. Failing on `main` before my branches, so unrelated — flag before other contributors trip over it.

**Medium:**
- Phase 5 cost tracking (design is ready per plan; needs migration + wrap + budget enforcement).
- `lqc_rerun_smoke_tests` + backend route (Phase 4b).

**Requires your decision:**
- Phase 7 write-path actions — you have explicit gating on approving this one.
- Whether to ship SSE subscriber or stay on 60s polling for the monitor task.

## Branch hygiene

Local worktree is on `claude/morning-report`. The parent worktree at `C:/Users/James/Desktop/LQ projects/Bot council` still has `claude/focused-raman-d6daa4` checked out — that's the branch this session started from. All feature branches I created were merged + `--delete-branch`'d remotely; locally they remain until you choose to `git branch -D <name>`.
