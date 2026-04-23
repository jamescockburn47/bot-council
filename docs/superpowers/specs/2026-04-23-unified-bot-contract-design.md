# Unified bot contract — design

**Date:** 2026-04-23
**Author:** James Cockburn (with Claude)
**Status:** Design / shipping in one PR
**Supersedes:** the external-vs-text_only distinction committed in [2026-04-22-text-only-bot-mode-design.md](./2026-04-22-text-only-bot-mode-design.md). Text-only mode stands; external mode's strict-schema smoke and token-mandate retire.

## Problem

The 5-round redesign ([spec](./2026-04-22-five-round-redesign-design.md)) shipped yesterday. Running it against the live fleet revealed that the committed two-contract setup was blocking every pre-existing external bot at preflight for reasons that have nothing to do with debate quality:

- **Clint** (localhost:3000) — no encrypted token in the DB because localhost endpoints never had one. The preflight `bot.token_ciphertext.is_none()` gate rejected him. He also couldn't be re-submitted through `/bots/submit` because `endpoint_url must use https://` refuses `http://localhost`.
- **Oscar** (trycloudflare) — same token-null rejection. Oscar's own `/debate` endpoint happens to be permissive about the `Authorization` header being absent, but we never got to test that because preflight short-circuited on the DB check.
- **Strict smoke schema** — the external-mode smoke test required `challenge` on round 2 and `position_change` on round 4 in the wire response. An external bot that returns prose only (like Clint does after its model change) fails smoke even though the council would happily extract those fields from the prose downstream. Extraction was gated on `bot_kind == "text_only"`, so external bots got zero extraction help despite the identical prose-handling capability existing three files over.

The text-only bot spec deliberately scoped simplification to new bots: *"external: existing path, no changes"*. That preserved backwards-compat but created an asymmetry — new bots write prose and we parse it, old bots must also emit structured fields. The asymmetry is not load-bearing. Extraction works the same on any prose. The decision to preserve two contracts was conservative; in practice it gates recovery of the existing fleet behind either direct DB surgery (what I did for Clint) or bot-owner rewrites (what was implicitly being asked of the others).

## Decision

One contract, applied to every bot regardless of `bot_kind`:

1. **Bots receive a prompt and return prose.** The wire shape still has room for `{response}` (external-style) or `{text}` (text-only-style) — the council accepts either, and the existing external-shape POST body continues to work for bots that parse it. No migration required on the bot side.
2. **We extract structured fields from prose.** Every response goes through the existing extractor for whatever fields are needed that round (`challenge` at R2, `crux_engagement` at R3, `steelman`/`position_change` at R4). Extraction runs on every bot. If the bot happened to supply the field in the response already, that short-circuits as `authored` — no MiniMax round-trip.
3. **Token is optional.** NULL token means "no Authorization header sent". Public bots that want auth set a token at submission; it's still stored encrypted.
4. **Endpoint URL accepts loopback addresses over HTTP.** `http://localhost*`, `http://127.0.0.1*`, `http://[::1]*` all valid. Public endpoints must still use HTTPS.
5. **Smoke validation accepts any non-empty prose.** No per-round structured-field check at smoke time. A bot that substantively answers the prompt passes; a bot that returns an empty body fails.
6. **Smoke per-request timeout is 180s** (was 60s). Tool-heavy bots like Clint do multi-step research per round — with the old timeout the first attempt regularly failed, triggering the smoke-test's one retry, overlapping two handlers on the bot side.

`bot_kind` stays on the schema as a hint to downstream observability (how the bot's owner positioned it) but no longer controls behaviour.

## Non-goals

- **Wire-shape unification.** The external shape (full `DebateRoundRequest` with `round`, `role`, `context`) and the text-only shape (flat `{prompt, session_id}`) both continue to work. Collapsing them into one would break every pre-existing external bot's handler. Not worth it — the prompt already carries everything a bot needs, and external bots that read extra fields still get them.
- **Retiring `bot_kind`.** Kept as a soft indicator. A follow-up PR can collapse it if the column proves redundant after a few weeks.
- **Migrating the five pre-existing bots' rows.** They stay as `external`. They just behave identically to a text-only bot now.

## Changes

### Backend

| File | Change |
|---|---|
| [src/api/debates.rs](../../../src/api/debates.rs) | Remove the `bot.token_ciphertext.is_none()` preflight gate. Dispatch already no-ops the `Authorization` header when token is NULL. |
| [src/api/bots.rs](../../../src/api/bots.rs) submit path | Drop the `token is required` reject. Accept `http://localhost`, `http://127.0.0.1`, `http://[::1]` — prefix match, any port, any path suffix. |
| [src/api/bots.rs](../../../src/api/bots.rs) smoke timeouts | Every `.timeout(Duration::from_secs(60))` → 180s. Four sites: text-only smoke probe, introduction probe, external smoke probe, top-level `smoke_test_bot`. |
| [src/api/bots.rs](../../../src/api/bots.rs) `validate_smoke_json_for_round` | Collapse to: body has `response` or `text` with non-empty string → OK. Drop the round-2 `challenge` check, round-4 `position_change` check, and the confidence type-check. `round` kept on the signature for call-site compat. |
| [src/orchestrator/extraction.rs](../../../src/orchestrator/extraction.rs) | Remove the `if bot_kind != "text_only"` short-circuits in `extract_crux_engagement`, `extract_steelman`, `extract_if_needed`. Extraction runs for everyone; if the response already has the field, provenance is `authored`, otherwise `extracted` (or `extraction_failed` on quote-verify miss). |
| [src/orchestrator/rounds/round2.rs](../../../src/orchestrator/rounds/round2.rs) | Remove the `!is_text_only && resp.challenge.is_none()` retry trigger. Never retry on missing challenge — extractor handles it. |

### Frontend

| File | Change |
|---|---|
| [frontend/src/routes/bots/submit/+page.svelte](../../../frontend/src/routes/bots/submit/+page.svelte) | Token marked optional. Help text updated: leave blank for localhost / private tunnels. Submit button enables regardless of token. |

### Tests

| File | Change |
|---|---|
| [tests/api_debates_test.rs](../../../tests/api_debates_test.rs) | `debate_creation_rejects_null_token_bot_always` → `debate_creation_does_not_reject_null_token_bot_at_preflight`. Asserts the error no longer cites a missing token. |
| [src/orchestrator/extraction.rs](../../../src/orchestrator/extraction.rs) tests | `external_bot_is_never_extracted` → `external_bot_with_existing_field_is_not_extracted`. Asserts that external bots that author structured fields keep `authored` provenance. |

### Clawdbot

| File | Change |
|---|---|
| [src/debate-handler.js](../../../clawdbot/src/debate-handler.js) (separate repo) | Revert the smoke-fastpath added yesterday. Now that smoke's per-request budget is 180s, Clint's tool-heavy research fits, so tools run in smoke too — same as in real debates. |

## Risks

- **Extraction cost.** Running extraction on every external bot's response adds a MiniMax call per field per bot per round. For a 5-bot / 5-round debate with `challenge` on R2, `crux_engagement` on R3, `steelman` and `position_change` on R4: 5 × (1 + 1 + 2) = 20 extra MiniMax calls. At MiniMax's throughput that's sub-dollar per debate. Acceptable; flagged for observability.
- **Extraction failures visible in transcript.** When extraction fails (quote not a substring, MiniMax rate-limited), the provenance is `extraction_failed` and the field is absent. The transcript UI shows this as an absence rather than a badge, which is the existing behaviour for text-only bots. No new UX state.
- **Bots that rely on token-required behaviour.** None, in practice — the token was a transport detail. A bot that wanted to reject unauthenticated callers (e.g. Oscar) can still do so; the council just sends whatever is in the DB (possibly nothing) and lets the bot respond.

## Rollout

1. Ship the PR; standard `./scripts/ship.sh` flow.
2. Verify preflight against the live fleet: Clint and Oscar should now pass without DB surgery. Alice passes (token already set). LQClaw + Akechi remain blocked on owner-side connectivity issues (tunnel + DNS).
3. The DB surgery I did for Clint yesterday (direct AES-256-GCM ciphertext INSERT) is harmless post-ship; the encrypted token is still valid and gets sent. No cleanup required.
4. No migration. No env change.
