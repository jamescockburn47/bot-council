# Operator legibility: the council explains itself — design

**Date:** 2026-07-02
**Author:** James Cockburn (with Claude)
**Status:** Design — awaiting implementation plans (phased)
**Related:** [2026-07-02-bot-lifecycle-design.md](./2026-07-02-bot-lifecycle-design.md) (Part 3's guidance table and Part 5's health model are consumed here); the sentinel inventory (`docs/sentinels.md`); operational lessons 14 and 16.

## Premise (binding)

The maintenance model is **operator + AI agents** — no staff engineer. Every
legibility artifact therefore has two readers at once:

- **The operator** needs plain English: what happened, what it means, what to
  do — at a glance, without asking anyone.
- **A maintaining agent** needs stable IDs, timestamps, and reproducible
  context — enough to land at the right invariant without archaeology.

These must be the **same artifact**. Every operator-facing message carries
its machine-greppable handle, so the escalation path for anything the
operator does not understand is: copy the card, paste it to an agent.

### The self-repair boundary (binding)

**Agents propose; gates dispose. The running system never edits itself.**
Any model (council-side MiniMax, Claude, or otherwise) may diagnose from the
journal and draft a fix — as a PR through the same CI gates as everyone
else, shipped by `ship.sh`, reversible by `rollback.sh`. Hot-patching the
live system is architecturally excluded: EVO holds no git repo, the binary
is the deploy unit, and applied-migration checksums make ad-hoc edits
boot-fatal. "Fixes on the fly" means minutes-to-PR, not seconds-to-mutation.

The same rule governs the journal itself: **narratives are authored
templates, deterministic and auditable — never LLM-generated as the source
of record.** An optional, clearly-labelled "explain this in more depth"
action may call the analysis model for elaboration; the stored record never
depends on it.

## Problem

Diagnostics today are engineer artifacts: `tracing` logs on EVO, sentinel
warnings in journald, Sentry events, and failure states that surface only
as a failed debate or an empty synthesis. The operator can run any command
but has no way to know *which* command or *why* without starting an agent
session — including for the question "is everything OK?", which should be a
glance. Architecture knowledge lives in CLAUDE.md and specs written for
agents, not for the owner.

## Decision summary

1. **The ship's log**: a `system_events` table + plain-English journal —
   the single load-bearing piece; everything else renders from it.
2. **The status page** (`/admin/status`): per-subsystem plain-English state
   with paste-ready remedies.
3. **"How this works"**: an operator-level architecture page, generated
   where possible, narrative where necessary.
4. **The runbook** (`docs/RUNBOOK.md`): the six real incidents, each as
   symptom → command → verification → agent handoff.
5. **The escalation card**: every problem renders a copy-for-an-agent block.

## Part 1 — The ship's log

### Data model

New table (additive migration):

```sql
CREATE TABLE system_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    severity TEXT NOT NULL,          -- 'info' | 'attention' | 'problem'
    event_kind TEXT NOT NULL,        -- stable, greppable (see catalogue)
    narrative TEXT NOT NULL,         -- plain English, authored template
    suggested_action TEXT NULL,      -- plain English, when one exists
    technical_detail TEXT NULL,      -- JSON: IDs, sentinel refs, raw error
    debate_id TEXT NULL,             -- when event is debate-scoped
    bot_id TEXT NULL                 -- when event is bot-scoped
);
CREATE INDEX idx_system_events_created ON system_events(created_at);
```

### The event catalogue

One authored module (`src/observability/system_guidance.rs`, mirroring
`error_guidance.rs`): for each `event_kind`, a severity, a narrative
template, and a suggested action. Initial catalogue, derived from the
existing warn-sites and operational history:

| Kind | Severity | Narrative gist |
|---|---|---|
| `debate_failed` | problem | "Debate X could not finish: <reason in plain terms>." |
| `synthesis_fallback` | attention | "The summary for debate X couldn't be structured; a simplified version was stored. The debate itself is intact. Re-running the summariser usually fixes this." |
| `synthesis_retry` | info | "The summariser needed a second attempt for debate X." |
| `sentinel_violation` | attention | "A built-in self-check (<ID>) flagged unexpected output in <context>. Nothing was blocked; the flagged item is marked." |
| `bot_unreachable` | attention | "Bot Y has been unreachable since <time>." (from lifecycle heartbeat) |
| `bot_salvage_heavy` | info | "Bot Y's responses needed salvaging in debate X (<kind>)." |
| `model_route_changed` | info | "The analysis model route changed to <url/model>." (detected at boot vs previous boot's recorded route — the lesson-14 drift alarm) |
| `service_started` | info | "The council restarted (version <sha>)." |
| `resynth_run` | info | "Summaries were rebuilt for N debates." |
| `quorum_not_met` | problem | "Debate X was cancelled: fewer than 3 bots were reachable." |

Adding an event kind = one catalogue entry + one `record_event(...)` call;
a freshness-style test asserts every kind used in code exists in the
catalogue (the ING-001 closed-set pattern).

### Writers

A small `pub fn record_event(pool, kind, params...)` in
`src/observability/` (fire-and-forget: an event-write failure logs and
never breaks the operation being recorded). Calls added at the existing
significant `tracing::warn!` sites — synthesis fallback/retry/salvage,
sentinel `log_violations` (also records an event), debate failure/quorum,
boot (service_started + model-route comparison), resynth CLI. Tracing logs
remain untouched — the journal is additive, not a logging replacement.

### API

`GET /api/admin/events?limit=50&severity=problem` (RequireAdmin) — newest
first, filterable. Serves the status page and any agent that wants the
journal directly.

## Part 2 — The status page

`/admin/status` (RequireAdmin), one glance, plain English. Subsystem rows:

| Subsystem | Check | Plain-terms framing |
|---|---|---|
| The web app | trivially up if the page rendered | "The site itself." |
| The database | `SELECT 1` + last write timestamp | "Where debates and bots are stored." |
| The summariser | last synthesis outcome from the journal + a cached reachability probe of the configured analysis route (never more than one probe per 5 minutes) | "The AI service that writes debate analyses and summaries." |
| The bot fleet | health badges from the lifecycle model (green/amber/red counts); until lifecycle Phase 4 ships, this row falls back to each active bot's most recent dispatch outcome | "The debaters." |
| The public doorway | self-fetch of `https://lqcouncil.com/api/health` (cached, 5-min) | "The tunnel that makes the site reachable from the internet." |
| What's running | `SENTRY_RELEASE` SHA vs latest known main (recorded at deploy); `.prev` presence via a deploy-time marker | "Which version is live, and whether an instant rollback is available." |

Each row: one sentence of what the part *is*, current state, and — when
degraded — the remedy: what to do in plain terms, the exact command to
paste, and what "fixed" looks like. Remedies come from the runbook and are
linked to it. Below the grid: the journal's recent entries with severity
chips, `problem` entries pinned to the top while unresolved (a `problem` is
"unresolved" until a newer event of the same kind+scope reports recovery,
or 24h pass).

The page never requires interpretation: green = look away; amber = read one
sentence; red = a remedy block with a copy button.

## Part 3 — "How this works" (operator-level architecture)

A page (route `/admin/how-it-works`, content also committed as
`docs/OPERATOR-GUIDE.md`) explaining the system at owner altitude:

- The request chain in one diagram and one paragraph: browser → Cloudflare's
  tunnel → the app on the EVO box → SQLite database + the analysis model.
- One short section per subsystem: what it does, what depends on it, what
  breaks when it's down, where its events appear in the journal.
- What happens during a debate, start to finish, in ten plain sentences.
- Where the data lives and what backs it up.

**Anti-drift:** the mechanical parts are generated with freshness tests
(the sentinel-inventory pattern): the sentinel table (already generated),
the event-kind catalogue, and the effective model routing (from the same
source as `/api/diag/models`). Narrative prose is hand-maintained under the
docs-drift-is-a-defect rule; each narrative section names the code/config
it describes so an agent can verify it in review.

## Part 4 — The runbook

`docs/RUNBOOK.md`, linked from every degraded status row. Six incidents,
one page, each in four moves — symptoms in plain terms → one command to
paste → how to verify → what to hand an agent if it didn't work:

1. **The site is down or erroring** → `./scripts/rollback.sh` (via the ship
   anchor) → `curl https://lqcouncil.com/api/health` returns ok → escalation
   card from the journal.
2. **Summaries are failing or vapid** (MiniMax outage) → the documented
   env-route fallback to the local model + restart → next debate's synthesis
   succeeds → card.
3. **The public site is unreachable but EVO is fine** (tunnel) →
   `sudo systemctl restart sovren-cloudflared` → health URL → card.
4. **A deploy is needed** → `./scripts/ship.sh` from the anchor → the
   seven-stage output ends green → card.
5. **Summaries need rebuilding** (after prompt changes) →
   `bash /home/james/resynth-launch.sh` on EVO → journal shows
   `resynth_run` → card.
6. **EVO rebooted / everything cold** → what auto-starts (systemd units),
   what to check in order (status page top-to-bottom) → card.

The runbook states its own maintenance rule: any PR that changes a command
it names must update it (reviewable by agents; the file is small enough to
diff by eye).

## Part 5 — The escalation card

Every `problem` journal entry and every red status row renders a
"Copy for an agent" block: a self-contained task statement containing the
event kind, severity, timestamps, scoped IDs (debate/bot), the sentinel or
error-kind handle, the relevant runbook section, and one line of what the
operator already tried. Phrased as an instruction ("Investigate why…"), not
a log excerpt. This is the operator→agent handoff made mechanical: no
context reconstruction, no "can you look at EVO", no lost detail.

## Model note

The council-side analysis/synthesis route is moving to MiniMax M3. This
spec is model-agnostic: the route change is the usual `APP__MODELS__*` env
override (lesson 14 applies — probe the env file first), and the journal's
`model_route_changed` event exists precisely to make such changes visible
in-product rather than discoverable by grep.

## Error handling

| Failure | Behaviour |
|---|---|
| Event write fails | Logged at warn; the recorded operation proceeds — the journal never breaks the thing it narrates |
| Unknown event kind reaches `record_event` | Recorded with a fallback narrative + flagged by the catalogue closed-set test in CI |
| Status probe times out | Row shows "couldn't check just now" (amber), never blocks the page; probes are cached and rate-limited |
| Self-fetch of the public URL fails from EVO | Distinguishes "tunnel down" from "the checker itself failed" by also checking localhost:3100 |
| Journal grows unbounded | Retention: `info` pruned after 30 days, `attention` 90, `problem` kept forever (they are the incident history); nightly prune task |

## Testing

- Catalogue closed-set test (every kind used in code has an entry).
- `record_event` failure-isolation test (poisoned pool → operation still
  succeeds).
- Narrative template tests: every catalogue entry renders non-empty
  narrative and, for `problem`/`attention`, a suggested action.
- Status endpoint integration test with mocked probes (each subsystem state
  renders; degraded rows carry remedies).
- Freshness tests for the generated architecture sections (existing
  sentinel pattern).
- Runbook: a CI grep asserts every command named in RUNBOOK.md exists in
  `scripts/` or is marked `[manual]` — commands can't silently rot.

## Risks

- **Narrative rot** — templates describing behaviour that changed. Contained
  by the closed-set test, the docs-drift rule, and keeping narratives about
  *observable outcomes* ("a simplified summary was stored") rather than
  implementation detail.
- **Status page as a second monitoring system drifting from Sentry** — it
  is not a replacement: Sentry stays the deep diagnostic; the status page is
  the owner's glance. The journal links Sentry event IDs in
  `technical_detail` where available.
- **Alert fatigue via `attention` noise** — severities are deliberately
  coarse and the default view collapses `info`; if `attention` proves noisy
  the fix is recataloguing kinds downward, a one-line change each.
- **Probe cost/flapping** — all probes cached ≥5 minutes, two-failure
  debounce (the heartbeat pattern).

## Phasing

| Phase | Scope | Ships as |
|---|---|---|
| 1 | `system_events` + catalogue + `record_event` at existing warn sites + events API + RUNBOOK.md | One PR (backend + docs) |
| 2 | Status page + journal UI + escalation cards | One PR (frontend + probe endpoints) |
| 3 | "How this works" page with generated sections + OPERATOR-GUIDE.md | One PR |

## Non-goals

- Autonomous self-patching of the running system (excluded by the binding
  boundary above).
- LLM-generated narratives as the record (elaboration action only, later).
- Replacing Sentry, tracing, or journald.
- Alerting/notification channels (the lifecycle spec's v1 stance holds).
- Public/user-facing status page (operator-only; revisit if the community
  asks).
