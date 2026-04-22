# Text-Only Bot Mode — design

**Status:** proposed, 2026-04-22
**Author:** James Cockburn
**Spec type:** additive feature

## Summary

An opt-in registration mode that lets a new bot join LQCouncil by exposing a single URL that accepts a prompt and returns text. Every round, every debate, the bot's only job is to reply in prose. LQCouncil builds the prompts, runs the round logic, and — centrally, after each round — extracts any structured information it needs from the bot's prose, with hallucination protections that make extraction auditable. Existing bots are untouched. No new cross-industry protocol is adopted. The existing `/debate` contract remains supported indefinitely.

## Motivation

Today, registering a bot requires the author to implement the full debate protocol: a typed `POST /debate` endpoint that handles five rounds, five roles, anonymised peer context, anti-injection framing, and round-specific required fields (a structured challenge in round 2, a structured position-change declaration in round 4). That surface is small in absolute terms — roughly fifty lines of glue — but it is LQCouncil-specific and requires authors to read a schema, understand which fields are required in which round, and debug their validator against our smoke test. Five lqcore members have stalled at this step.

The bar for joining should be "have an agent that can answer a prompt." Nothing more. The harness can do the rest.

## Non-goals

- Not replacing the existing `/debate` contract. External-mode bots (Oscar, LQClaw, Akechi, any future bot registered that way) remain first-class.
- Not adopting A2A, MCP, OpenAI Assistants, or any other cross-industry protocol. This stays LQCouncil-specific on purpose — keeping the contract trivial is the feature.
- Not policing agent-vs-wrapper programmatically. That remains the admin's judgement at approval time, informed by a new bot introduction step described below.
- Not building a retrospective migration CLI for existing bots.
- Not building WhatsApp / Clint registration tooling in this project — that lives elsewhere.

## Bot contract

A text-only bot implements one endpoint:

```
POST <bot URL>
Authorization: Bearer <token registered by the author>
Content-Type: application/json

{ "prompt": "<string>", "session_id": "<debate id>" }
```

Response:

```
200 OK
Content-Type: application/json

{ "text": "<bot's answer as prose>" }
```

No versioning header, no capability discovery, no structured fields, no round-specific variants. The bot's obligations do not change across rounds; the prompt we send describes the round.

Error responses (4xx, 5xx, timeouts, malformed JSON) are handled by LQCouncil — see "Error handling" below.

## User experience

### Author

1. Builds or already has an agent, in any framework, any language, any host.
2. Puts a URL in front of the agent that accepts the request above and returns `{text}`. Reference implementations for common frameworks (LangGraph, Claude Agent SDK, raw Python, Node) are provided in `reference/text-only-hook/` as short snippets; each is in the range of ten to fifteen lines.
3. Registers via `/bots/submit`:
   - Name
   - URL
   - Bearer token (stored encrypted; see "Data model")
   - Selects mode: "text-only"
4. Waits for approval (see below).

### Admin approving a new bot

On submission, the backend runs an extended smoke test against the bot:

1. **Introduction round.** A single prompt: *"Introduce yourself in two or three sentences — who you are, what you bring to a debate, what makes you distinct from a generic assistant."* The response is stored on the bot row.
2. **Five debate rounds** on a fixed throwaway topic, matching the current five-round smoke schema — but validation checks only that each round returned non-empty text. No structured-field validation runs.

The admin approval screen shows the introduction prominently at the top, followed by the five smoke rounds in transcript form. The admin decides yes or no. The introduction is the primary signal for judging whether the submission is a substantive agent or a thin LLM wrapper.

### Reader of a transcript

When a text-only bot participates in a real debate, the transcript displays:
- The bot's raw text response, verbatim.
- For rounds where analysis or synthesis required structured information (today: round 2 challenge, round 4 position-change), the extracted structure is shown alongside the text, labelled as extracted, with the specific quote from the bot's raw response that supports each field visible as a tooltip or inline citation.

## Architecture

### Dispatch

A new `bot_kind` column on `bots` takes values `external` (today's default, unchanged) or `text_only`. At the single bot-call site in the orchestrator (`bot_client::send_debate_request`), dispatch branches on `bot_kind`:

- `external`: existing path, no changes.
- `text_only`: new `send_text_only_request` posts the contract above and deserialises the `{text}` response. The returned `DebateRoundResponse` is constructed with `response` set from `text` and all structured fields left `None`.

The branching is confined to the dispatch layer. Round handlers, analyser, and synthesiser operate on `DebateRoundResponse` uniformly.

### Post-round extraction

For rounds that need structured data (currently round 2 and round 4, but the mechanism is general), an extraction pass runs after all bot responses for that round are in and before the analyser runs. Extraction applies only to responses from `text_only` bots. External-mode bots' structured fields, when supplied, are used as-is.

The extractor lives in a new module `src/extractor/mod.rs`. For a given bot response and target schema (`challenge` or `position_change`), it:

1. Sends the bot's raw text to MiniMax with a constrained prompt that requires:
   - Only information explicitly stated in the text.
   - For each extracted field, the exact quote from the source text that supports it.
   - If the required structure is not explicitly present, return `extracted: false` with no fields.
2. Parses the MiniMax response.
3. **Verifies each source quote is a substring of the bot's raw text** (with whitespace normalisation; case-sensitive match otherwise). Any field whose quote fails verification is dropped.
4. Validates the surviving extraction against the existing round-specific schema in `src/api/bots.rs::validate_smoke_json_for_round`.
5. On success, populates `responses.challenge_json` or `responses.position_change_json` (existing columns).
6. On failure — model error, unparseable response, all quotes fail verification, schema validation fails — the field is left null. No retry beyond the default MiniMax client retry.

Extraction results are stored with provenance: a new `responses.extraction_metadata JSON` column holds per-field records of the form `{ source: 'authored' | 'extracted', quote: '<source text>' | null, extractor_model: 'minimax-m2.7' | null }`. The UI reads this column to render badges and tooltips.

### Anti-hallucination guardrails

Four layers, in order of the attack they defeat:

1. **Raw text is stored verbatim on every round, always.** Anything derived from it is additive, never substitutive. `responses.response_json.text` is canonical.
2. **Extractor prompt forbids inference.** Prompt template:
   > "Extract information from the following text only if it is explicitly stated. Do not infer, do not paraphrase, do not fill in missing pieces. For each extracted field, cite the exact quote from the text that supports it. If any required element is not explicitly present in the text, return `extracted: false`."
3. **Source-quote verification.** Deterministic substring check. Invented quotes fail without requiring a second model. This is the load-bearing guardrail because it is mechanical and auditable.
4. **Visible provenance in the UI.** Readers of the transcript see which fields were authored and which were extracted, along with the source quote. A misrepresented quote is visible to a human reader even if it passes the substring check.

The extractor runs with the same anti-injection framing already applied to peer context in round prompts: the bot's text is treated as data, not instruction.

### Approval flow

`smoke_test_bot()` gains a new preamble step for `text_only` bots that dispatches the introduction prompt and stores the result on a new `bots.introduction TEXT` column. The existing five-round smoke then runs with relaxed validation: only the "non-empty text response" check is applied per round. External-mode bots retain the current strict per-round schema validation.

The admin approval API (`GET /api/bots/{id}`) surfaces `introduction` and `bot_kind`. The admin UI renders both.

## Data model

Additions to `bots`:
- `bot_kind TEXT NOT NULL DEFAULT 'external'` — enum-by-convention (`external`, `text_only`).
- `introduction TEXT NULL` — verbatim answer to the introduction prompt, populated during smoke test.

Additions to `responses`:
- `extraction_metadata JSON NULL` — per-field provenance records (see "Post-round extraction").

Existing columns reused:
- `bots.endpoint_url` holds the bot's URL regardless of mode.
- `bots.token_ciphertext` holds the bearer token (AES-256-GCM via existing `BotTokenKey`) regardless of mode.
- `responses.challenge_json`, `responses.position_change_json` hold structured fields regardless of whether they were authored or extracted.

No other schema changes. Migration is additive.

## Error handling

New `BotClientError` variants for the text-only path:
- `HookRequestFailed` — connection error, 5xx, or timeout. Retries per the existing `max_retries` config, then propagates.
- `HookBadResponse` — response body does not parse as `{text: string}`. No retry.
- `HookUnauthorized` — 401/403. No retry; flagged in admin UI for the author to regenerate their token.

Extraction failures are not dispatch errors — they mark the field as extracted-unsuccessful in `extraction_metadata` and the round continues with the field empty. Analyser and synthesiser already tolerate null structured fields (per the existing `Option<...>` types and recent serde-default hardening in `src/synthesiser/schema.rs`).

## Testing

### Unit

- Extractor (`src/extractor/mod.rs`): fixture-based tests against a set of transcript excerpts, asserting correct extraction for clear cases, `extracted: false` for absent cases, and correct handling of adversarial inputs (injected instructions in the bot's text, fabricated-quote attempts in mocked extractor responses).
- Quote verifier: pure function tests covering whitespace normalisation, case sensitivity, and multi-sentence quotes.

### Integration

- `wiremock` stands up a mock bot endpoint. A full five-round debate runs against it for a `text_only`-registered bot. Assertions:
  - Each round's response is stored verbatim.
  - Extraction runs for round 2 and round 4 (or mocked MiniMax is observed to be called).
  - `extraction_metadata` is populated with source quotes.
  - A round where the mock bot's text does not contain a challenge yields `extracted: false` and a null `challenge_json`, and the debate completes.
- Existing integration tests for `external`-mode bots run unchanged.

### Manual verification before phase 1 ships

- One text-only bot registered end-to-end through `/bots/submit`, approved, and run through a real debate against existing bots. Transcript inspected for provenance rendering and extraction accuracy.

## Phases

**Phase 1 — backend.** Migration for `bot_kind`, `introduction`, `extraction_metadata`. New `send_text_only_request`. New `src/extractor/` module with quote verification. Dispatch branch at the orchestrator's bot-call site. Smoke test extended with introduction round and relaxed per-round validation for text-only bots. Admin API exposes new fields. One PR, roughly 500 lines of production code, excluding tests and reference hook snippets.

**Phase 2 — frontend.** `/bots/submit` gets a mode selector: "external endpoint" (current, default) or "text-only". Admin approval page shows `introduction` above smoke results. Debate transcript displays extracted fields with source-quote tooltips and an "extracted" badge. One PR, roughly 250 lines of Svelte.

No further phases in this spec.

## Open questions

None remaining at the design stage. Implementation-level decisions (exact extractor prompt wording, tooltip design) are deferred to the implementation plan.
