# Bot lifecycle: connection, onboarding, and monitoring — design

**Date:** 2026-07-02
**Author:** James Cockburn (with Claude)
**Status:** Design — awaiting implementation plans (phased)
**Supersedes:** the onboarding portions of [2026-04-23-unified-bot-contract-design.md](./2026-04-23-unified-bot-contract-design.md) (wire contracts from that spec remain in force); operational lesson 18's dependence on Clint's `lqc_*` validation tools.
**Related:** [2026-07-02-issue-centric-sessions-design.md](./2026-07-02-issue-centric-sessions-design.md) — independent; both compose. This spec is transport/lifecycle; that one is protocol/output.

## Principles (binding for this spec)

1. **The floor is a connector that cannot be got wrong.** Any prose-shaped
   response counts. A bot author's irreducible obligations are: answer the
   prompt, be substantive, be invocable. Everything else is council-side.
2. **Every failure is visible in plain English with a fix suggestion.** An
   owner never needs the operator (or Clint) to diagnose their bot.
3. **Optimisation is guidance, never a gate.** A bot that ignores every
   suggestion but responds reliably stays green.
4. **Self-serve end to end.** Register, connect, verify, submit, monitor —
   without admin involvement until the approval decision itself.

## Problem

Onboarding stalls on operations, not code: the wire contract is already a
15-line wrapper, but authors must run a 24/7 public HTTPS endpoint
(tunnel/DNS/TLS), and the fleet's real blockers (LQClaw, Akechi) were dead
tunnels — invisible until preflight failed at debate-creation time.
Diagnosis is not self-serve: token mismatches surface as
`schema_missing_field`, smoke results are admin-only, and the error taxonomy
lives in code rather than in front of the owner. The smoke/preflight timeout
asymmetry (180s vs 25s) lets a bot pass approval and then fail every debate.

## Decision summary

1. **Pull-mode transport**: bots may connect out via long-polling; a
   single-file connector script carries all LQCouncil-specific knowledge.
2. **Lenient ingest**: a normalisation ladder accepts anything prose-shaped
   on both transports; oversize is truncated, never rejected.
3. **Honest errors**: a dedicated `auth` error kind; every error kind gets an
   owner-facing description and a fix hint in one table.
4. **Onboarding wizard**: register → connect → live checks, self-serve, with
   per-check plain-English results before submission.
5. **Owner monitoring page**: health badge, dispatch history in plain
   English, rates, wrapper signals, and a rule-derived optimisation panel.
6. **Heartbeat**: connection-level reachability probes for push bots;
   presence-by-polling for pull bots; instant presence preflight.

## Part 1 — Transports

### Push (existing, unchanged)

Both wire shapes (`DebateRoundRequest` and flat `{prompt, session_id}`)
continue to work exactly as today. Nothing in this spec breaks any bot in
the current fleet.

### Pull (new)

The bot connects out; the council never needs to reach it.

```
GET /api/bot-work?wait=25          Authorization: Bearer <bot token>
  → 200 {job_id, session_id, prompt, deadline}   (a job was available)
  → 204                                          (no work within `wait` seconds)

POST /api/bot-work/{job_id}        Authorization: Bearer <bot token>
  {"text": "<the bot's answer>"}   (lenient ingest applies — see Part 2)
  → 200
```

- **Long-polling, deliberately.** `wait` is capped at 25s (safe under
  Cloudflare's edge budget); the connector re-polls in a loop. Implementable
  in any language's stdlib; no WebSocket/SSE machinery in v1.
- **Token is mandatory for pull bots** — it is the bot's identity. Stored
  AES-256-GCM encrypted as today. Push bots keep optional tokens.
- **Job lifecycle.** New `bot_jobs` table: `id`, `bot_id`, `debate_id`,
  `round_label`, `prompt`, `status` (`queued | leased | done | expired`),
  `created_at`, `deadline`, `leased_at`, `result_text`. A poll leases the
  oldest queued job for the bot; posting the result completes it. Dispatch
  enqueues and awaits completion (tokio notify) inside the existing per-round
  300s budget; on deadline the job is marked `expired` and the round follows
  the normal retry → carry-forward ladder. One lease per job — a job that
  expires under lease is not re-queued (the dispatch retry ladder, not the
  queue, owns retries).
- **Presence.** Every poll updates `bots.last_poll_at`. Preflight for pull
  bots is `last_poll_at` within the last 60s — instant, no network probe, and
  the Cloudflare 100s preflight squeeze disappears for pull bots.
- **Dispatch branch.** `bot_client` gains a `pull` arm keyed on the new
  `bots.bot_transport` column (`push` default, `pull`). Round handlers,
  extraction, and synthesis are untouched — the transport is invisible above
  the dispatch layer.

### Connector scripts

`reference/connector/lqc-connect.py` and `lqc-connect.js` — single-file,
stdlib-only (~60 lines each), no package publishing in v1:

```
LQC_TOKEN=<token> python lqc-connect.py --cmd "python my_agent.py"
LQC_TOKEN=<token> node lqc-connect.js --url http://localhost:8000
```

`--cmd` pipes the prompt to a shell command's stdin and reads the answer
from stdout; `--url` forwards to a local text-only hook. The script owns
polling, job handling, retries, and backoff. Contract evolution becomes a
connector version bump instead of a fleet-wide coordination problem — the
failure mode of operational lesson 18, retired.

## Part 2 — Lenient ingest (both transports)

A normalisation ladder replaces strict response parsing. In order:

1. JSON object: first non-empty string among `text`, `response`, `output`,
   `content`, `message`, `answer`.
2. OpenAI envelope: `choices[0].message.content`.
3. Raw body as prose (non-JSON or unrecognised JSON shape).
4. Strip wrapping code fences and `<think>…</think>` blocks.
5. Oversize (> 20 KB): **truncate to the limit and continue** — recorded,
   never rejected.

Only a genuinely empty result is an abstention. The path taken is recorded
per response as `ingest_kind` (`clean | salvaged_field | salvaged_raw |
truncated`) in the responses metadata — a monitoring-page quality signal,
never a round failure. The old `json_parse`, `schema_missing_field`, and
`schema_invalid_*` error kinds stop being reachable from response-shape
issues (they remain in the enum for historical rows).

**Verbatim storage is preserved:** the raw body is stored as received;
normalisation is additive, same pattern as extraction provenance.

**New sentinel `ING-001`** ("a non-empty response body never produces an
ingest error") joins `docs/sentinels.md` with a validator at the ingest
seam, per the sentinel inventory conventions.

## Part 3 — Honest errors

- **New error kind `auth`.** HTTP 401/403 from a push bot maps to `auth`,
  not `schema_missing_field` (today's misclassification). Owner-facing text:
  "Your endpoint rejected the council's credentials — check that your bot
  uses the token you registered."
- **One table, three consumers.** `error_kind.rs` gains, per kind: an
  owner-facing plain-English description and a fix hint. The wizard's live
  checks, the owner monitoring page, and the admin views all render from
  this single table — no drift between surfaces.

## Part 4 — Onboarding wizard (`/bots/submit` rework)

Three steps, each self-serve:

1. **Register.** Name + transport choice: "Run our connector (recommended —
   works from any machine, no public URL)" or "I have an endpoint URL". The
   bot row is created immediately as `pending` with its token issued, so the
   author can connect and test before any admin involvement.
2. **Connect.** Pull: a copy-paste connector command with the token inlined.
   Push: URL field, the existing reference hook snippets, and the public-URL
   recipes from `/bots/guide`.
3. **Live checks.** Run in front of the author, each rendering pass / warn /
   fail with a plain-English fix line from the Part 3 table:
   - *Reachable* — push: connection succeeds; pull: "waiting for your
     connector's first poll…" flips green when it arrives.
   - *Responds* — a sample prompt round-trips; the author sees exactly what
     the council parsed and which ingest path fired ("we read your response
     via `salvaged_raw` — return `{"text": …}` to skip salvage" is a warn,
     not a fail).
   - *Latency* — warn above 120s with the timeout-budget explanation.
   - *Introduction preview* — the introduction probe runs and displays, so
     the author sees what the admin will see.
   Checks reuse the existing validate/smoke machinery server-side.

Submission and admin approval (introduction + 5-round smoke) are unchanged —
except smoke results become visible to the **owner** on their bot page, not
only to the admin.

## Part 5 — Owner monitoring page

Per-bot page reachable from my-submissions:

- **Health badge.** Computed on read from stored data — no stored health
  column. Over the last 10 dispatches plus contact recency:
  - **Red**: 2+ consecutive dispatch failures, or unreachable / not polling
    for over an hour.
  - **Amber**: any of — salvage rate above 20%, p50 latency above 120s, 1–2
    non-consecutive failures.
  - **Green**: everything else.
- **Last contact** (push: `last_reachable_at`; pull: `last_poll_at`).
- **Recent dispatches** — time, debate, round, outcome; failures show the
  plain-English description + fix hint; latency per dispatch.
- **Rates** — abstention rate, ingest-salvage rate, retry rate.
- **Wrapper signals** — the verbatim failure strings synthesis already
  extracts for abstaining bots, surfaced to the person who can act on them.
- **Optimisation panel** — rule-derived from the bot's own data:
  - timeouts → "tighten your internal budget to ~120s";
  - `salvaged_raw`/`salvaged_field` → "return `{text: …}` directly";
  - abstentions with wrapper signals → "check your upstream API key/limits";
  - repeated carry-forwards → "your bot isn't engaging with later rounds —
    check it handles long prompts";
  plus a static "what makes a strong council bot" guide (substance, cited
  sources, named-peer engagement, committed positions, latency headroom).
  Suggestions never gate anything (Principle 3).

Owner identity: pages are scoped to the Clerk user who submitted the bot
(existing my-submissions ownership model).

## Part 6 — Heartbeat and admin view

- **Push bots:** a background task probes each active push bot's endpoint
  every 15 minutes at the **connection level only** (TCP + TLS handshake, no
  request body) — zero cost to the bot's agent or LLM budget. Result updates
  `bots.last_reachable_at`. Dead tunnels and DNS rot surface as "unreachable
  since <time>" on the owner page instead of a preflight surprise.
- **Pull bots:** no probe needed; presence is `last_poll_at`.
- **Admin:** the `/bots` admin list gains health badges, last-contact, and a
  "degraded" filter. The manual smoke-test button remains.
- **No alerting in v1** — no email/push notifications; the badge and the
  owner page are the surface. Revisit if owners ask.

## Data model (all additive)

| Change | Purpose |
|---|---|
| `bots.bot_transport TEXT NOT NULL DEFAULT 'push'` | transport selector |
| `bots.last_poll_at TEXT NULL` | pull presence |
| `bots.last_reachable_at TEXT NULL` | push heartbeat |
| `bot_jobs` table (Part 1) | pull work queue |
| `responses.ingest_kind TEXT NULL` | salvage-path quality signal |

No changes to existing columns; no migration of existing rows.

## Error handling

| Failure | Behaviour |
|---|---|
| Pull bot stops polling mid-debate | Job hits deadline → `expired` → normal retry → carry-forward ladder; owner page shows the gap |
| Two connectors poll with the same token | Jobs lease one-at-a-time; both receive work interleaved — harmless duplication of capacity, documented as unsupported |
| Bot posts a result after job deadline | 409; the round has moved on; recorded on the job, shown on the owner page |
| Poll flood | Light rate-limit on `/api/bot-work` (per-token, generous); 429 with Retry-After |
| Heartbeat probe fails transiently | Two consecutive failures before `last_reachable_at` stops advancing (no flapping badges) |
| Ingest receives empty body | Abstention, as today — the one case lenient ingest does not save |
| Wizard live-check hangs | Each check has its own timeout and renders "timed out" with a fix hint; the wizard never blocks submission on a warn |

## Testing

- **Ingest ladder:** unit tests per rung (each field name, OpenAI envelope,
  raw prose, fenced/`<think>` stripping, truncation) + the `ING-001`
  sentinel validator tests; adversarial: instruction-shaped text in a salvaged
  body stays inert inside the existing data framing.
- **Pull transport:** integration test — register a pull bot, poll, receive
  a job, post result, assert the round consumed it; deadline-expiry test
  asserts the retry ladder fires; presence preflight test (fresh poll passes,
  stale fails).
- **Auth kind:** dispatch against a 401-ing wiremock asserts `error_kind =
  auth`, not `schema_missing_field`.
- **Connector scripts:** exercised in CI via a smoke script against a local
  mock (Python + Node available on runners); kept stdlib-only so no
  dependency install step.
- **Wizard/monitoring frontend:** svelte-check + build in CI; manual
  verification against one live bot per the deploy checklist.
- **Heartbeat:** unit test the probe state machine (two-failure debounce);
  integration with a droppable listener.

## Risks

- **Pull-mode queue correctness** is the substantive new machinery (leases,
  deadlines, notify-on-complete). Mitigation: it lives behind the dispatch
  seam with the retry ladder unchanged above it; property of the design is
  that a wedged queue degrades to the existing abstention path, never a hang.
- **Lenient ingest weakens smoke as a diagnostic** — a misconfigured bot gets
  silently salvaged forever. Mitigation: `ingest_kind` is recorded and
  surfaced as amber on the owner page and in the wizard ("we could read it,
  but…"), so quality pressure survives without gating (Principle 3).
- **Long-poll holds Axum connections** — ~one held connection per idle pull
  bot. Trivial at current fleet size; note for capacity if the fleet grows
  100×.
- **Token-in-command-line** (connector copy-paste) can end up in shell
  history. The wizard shows the env-var form (`LQC_TOKEN=…`) and the docs
  say why; accepted residual risk for v1.
- **Two transports to maintain.** The cost of never breaking the existing
  fleet. Contained: the fork is one dispatch branch + preflight branch.

## Phasing

| Phase | Scope | Ships as |
|---|---|---|
| 1 | Lenient ingest + `auth` kind + fix-hint table + `ING-001` | One PR (backend) — the reliability floor |
| 2 | Pull transport: `bot_jobs`, poll/complete endpoints, dispatch branch, presence preflight, connector scripts | One PR series (backend + reference/) |
| 3 | Onboarding wizard | One PR (frontend, reuses validate machinery) |
| 4 | Owner monitoring page + heartbeat | One PR series (backend task + frontend page) |
| 5 | Admin fleet polish (badges, degraded filter) | Small PR (mostly reuse) |

Phases 1–2 deliver the "simple reliable connector"; 3–5 deliver clear-and-
easy. Each phase gets its own implementation plan against this spec.

## Non-goals

- OpenAI-compatible endpoints as a third wire shape (cheap complement,
  deferred to its own small spec once the ingest ladder exists).
- WebSocket/SSE transport, connector package publishing (npm/PyPI), email or
  push alerting, per-bot trust flags.
- Updating Clint's `lqc_*` tools — the wizard and owner page replace the
  need rather than porting the legacy path.
- Any change to round protocols, extraction, or synthesis (the sessions spec
  owns those).
