# Issue-centric sessions: output schema, layered reading path, map rework, and multi-mode generalisation — design

**Date:** 2026-07-02
**Author:** James Cockburn (with Claude)
**Status:** Design — awaiting implementation plan
**Depends on:** [five-round redesign](./2026-04-22-five-round-redesign-design.md) (shipped), [unified bot contract](./2026-04-23-unified-bot-contract-design.md) (shipped), [outcome tab](./2026-04-19-debate-outcome-tab-design.md) (shipped — parts are superseded here)
**Related, separate track:** universal bot connector (lenient ingest / pull transport / OpenAI-compat) — discussed 2026-07-02, not in this spec.

## Product thesis (design principle, binding for this spec)

The council's value is **preserved disagreement, not resolved answers**. It is a
counterweight to single-model over-simplification: output must surface nuance,
counterpoints, minority reasoning, and frame-rejections — and must make them
*legible*, not compress them into a false verdict because verdicts are easier
to render. Every design decision below is tested against this principle.

The corollary that resolves the "overwhelming output" complaint: **nuance does
not require less detail; it requires narrative structure.** A court judgment is
deeply nuanced yet readable because it is organised as issues → holding per
issue → reasoning → dissents. The current output is organised as parallel
database lists. That, not the volume of content, is the overwhelm.

## Problem

1. **The synthesis schema has no narrative joins.** `SynthesisOutput`
   ([src/synthesiser/schema.rs](../../../src/synthesiser/schema.rs)) is five
   parallel lists — `consensus_points`, `live_disagreements`,
   `flagged_capitulations`, `minority_positions`, `confidence_trajectories` —
   plus two prose fields. The reader must mentally join them (is this minority
   position a side of that disagreement? did this capitulation settle that
   issue?). The UI faithfully renders the schema, so the UI reads as a data
   dump. Collapsing sections cannot fix a data model with no joins.
2. **The crux is computed and then buried.** The R3 redesign selects the
   debate's central disagreement (`crux_selection` analysis row) and classifies
   each bot's `crux_shift` — and `SynthesisOutput` has no crux field. The
   single most important narrative object ("the debate turned on X; two held,
   one conceded citing new evidence, one rejected the frame") never reaches the
   reader.
3. **The Outcome tab front-loads its full interactive surface.** Sub-tab
   pills, filter row, legend, force-directed map, and replay slider all render
   unconditionally, and the drawer opens into full-paragraph evidence with no
   one-line takeaway. Readers report overwhelm; diagnosis (user-confirmed):
   too many surfaces to choose between, and long unstructured prose.
4. **Generalisation is wanted but unproven.** Competitions and research groups
   are worth building only where a multi-bot council genuinely beats asking a
   single frontier model. That test (user-confirmed) is: distinct capability
   (different RAG/tools/models), adversarial stress-testing, and independent
   parallel attempts with comparative judging. The product currently asserts
   this value; it should *measure* it.

## Decision summary

1. Replace the list-shaped synthesis with an **issue-centric artifact schema**
   shared by all session types.
2. Rebuild the output page as a **four-layer reading path** generated from that
   schema; the map stays central and visually striking.
3. Rework the map to render the **argument's anatomy** (issues as anchors,
   positions as nodes, pressure as edges) instead of its taxonomy.
4. Add an optional **single-model baseline comparison** that measures the
   council delta per session.
5. Generalise debates into **sessions** with per-mode protocol modules:
   `debate` (existing), `competition`, `research`. All three terminate in the
   same artifact schema.
6. Add **session documents**: council-side parsing, BM25 retrieval, per-round
   excerpt injection, and quote-verified doc-claim policing.

Items 1–3 are the core; 4–6 are phased follow-ons sharing the schema.

## Part 1 — Issue-centric artifact schema

### Structure

New Rust types in `src/synthesiser/schema.rs` (file split if it exceeds the
300-line cap):

```rust
pub struct SessionArtifact {
    pub topic: String,
    pub headline: String,           // Layer 0: 1–2 sentence state of the argument
    pub executive_summary: String,  // retained: 4-sentence prose outcome
    pub issues: Vec<Issue>,
    pub baseline_delta: Option<BaselineDelta>,   // Part 4; None when toggle off
    pub meta_observations: String,  // retained
}

pub struct Issue {
    pub issue: String,              // the question, stated neutrally
    pub headline: String,           // 3–6 word label (graph-node + card title)
    pub is_crux: bool,              // true for the R3 crux issue
    pub status: IssueStatus,        // Settled | Split | Reframed
    pub positions: Vec<Position>,
    pub movement: Vec<Movement>,
}

pub struct Position {
    pub stance: String,             // full sentence
    pub headline: String,           // 3–6 word label
    pub bots: Vec<String>,          // pseudonyms
    pub best_argument: String,      // with citation
    pub evidence: String,
    pub final_confidence: Option<i64>,  // mean of holders, if reported
    pub frame_rejection: bool,      // "the dichotomy is false" — not a pole
}

pub struct Movement {
    pub bot: String,
    pub from: String,
    pub to: String,
    pub justified: bool,            // absorbs flagged_capitulations (false = flagged)
    pub trigger_quote: String,      // quote-verified against transcript
}
```

Every field carries `#[serde(default)]` — the existing MiniMax field-dropping
mitigation is retained wholesale.

### Semantics (how the old lists map)

| Old concept | New representation |
|---|---|
| Consensus point | `Issue { status: Settled, positions: [one] }` — bots listed on the surviving position |
| Live disagreement | `Issue { status: Split, positions: [two or more] }` |
| Minority position | A `Position` whose `bots.len() == 1` within a Split issue (or a Settled issue's lone dissent) |
| Flagged capitulation | `Movement { justified: false }` on the relevant issue |
| Crux outcome | `Issue { is_crux: true }` — always present when a `crux_selection` analysis row exists; `Reframed` status covers frame-rejection outcomes |
| Confidence trajectories | Dropped from the artifact. Movement + `final_confidence` carry the narrative; per-round confidence remains available from the transcript for the map's replay mode |

`positions` is not limited to two sides. A frame-rejection is a `Position`
with `frame_rejection: true` — rendered distinctly, never flattened into a
pole (product thesis).

### Supersede, not wrap

`SessionArtifact` **replaces** `SynthesisOutput`. No dual-write, no legacy
derivation layer. Migration is by resynthesis (the mechanism already exists
and is BINDING practice after prompt changes — operational lesson 16):

1. Ship backend + frontend together (frontend reads the new shape only).
2. Run `bot-council resynthesise --all` on EVO; historical debates are rebuilt
   into the new schema from their stored transcripts.
3. Debates whose resynthesis fails render the existing "synthesis not
   available" state with the Transcript tab intact — same degradation path as
   today.

The synthesis prompt in `src/synthesiser/mod.rs::build_synthesis_prompt` is
rewritten to request the issue structure directly, including the crux issue
(fed from the `crux_selection` analysis row and per-bot `crux_shift`
classifications, which currently exist but don't reach synthesis). Storage
stays in the existing `syntheses` table/columns — the JSON payload shape
changes, no migration needed for the core schema work.

## Part 2 — Layered reading path

Four layers; each is generated from `SessionArtifact`, and each answers the
question the previous layer raises. No layer previews another layer's content.

| Layer | Surface | Content | Budget |
|---|---|---|---|
| 0 | Landing block | `headline` (status-coloured, one clause per issue), `executive_summary`, baseline-delta strip when present | 10 seconds |
| 1 | Issue cards | One card per issue: status chip, `is_crux` marker, one line per position with holder pseudonyms, movement count. Click → Layer 2 drawer for that issue | 1 minute |
| 2 | The map | Central visual (Part 3). Node/edge click → drawer: **bolded one-sentence takeaway first**, support count, then evidence and quotes collapsed under "Show evidence" | 5 minutes |
| 3 | Transcript tab | Unchanged forensic scroll | unbounded |

### What is deleted from the Outcome tab

- The `DivergenceHeadline` five-tile grid and its 40px score bar (the issue
  cards' status chips carry the same signal in context).
- The standalone `ConfidenceChart` on the outcome surface (movement is on the
  map's replay; the chart stays available on the Transcript tab).
- The `Arguments | Positions` sub-tab pills — `BotStanceMap` content folds
  into the map's supporter-highlight interaction.
- The permanent filter row and permanent legend (move into one gear popover,
  closed by default).

Tab bar stays `Outcome | Transcript | Raw`. Default tab logic unchanged.

## Part 3 — Map rework: anatomy, not taxonomy

The map remains the centrepiece — larger canvas (reclaimed from deleted
chrome), zoom-to-fit on load.

### Model

- **Anchors are issues.** The crux issue renders largest, centred; other
  issues place around it. The topic is the page title, not a node.
- **Nodes are positions**, attached to their issue anchor, sized by holder
  count, coloured by issue status (settled emerald / split rose / reframed
  violet — palette continuity with today). Bot pseudonyms surface on
  hover/drawer, as today.
- **Edges are pressure**: rebuttal edges between opposing positions;
  concession edges (directional, from abandoned to adopted position) drawn
  from `movement`; **frame-rejection renders as a visibly broken edge** to the
  issue anchor — a rejected framing is not a midpoint between poles.
- **Replay slider = migration.** Scrubbing rounds animates bot attachment and
  position size using transcript-derived reconstruction (existing
  `reconstruct.ts` approach, retargeted at issues). Confidence trajectory
  becomes node-glow intensity over the scrub rather than a separate chart.
- **Legibility cap.** Above ~12 position nodes (competitions especially),
  lowest-support nodes collapse into an expandable "+N more" cluster per
  issue.

### Visual and interaction quality bar

The current map's failures are visual as much as semantic: crunched truncated
labels, awkward force-soup placement, perpetual jiggle. Phase 2 rebuilds the
presentation against these requirements:

- **2D, not 3D.** `ArgumentMap3D` is retired. 2D (SVG or canvas) with the
  same dark editorial aesthetic — glow, soft motion, depth cues — is the only
  way to get collision-aware labels and deterministic placement. Striking
  through polish, not through a third axis.
- **Labels are `headline` fields, never truncated prose.** The artifact
  schema's 3–6 word headlines are the node labels; full sentences live in
  hover tooltips and the drawer. No mid-word truncation anywhere.
- **Collision-aware label placement.** Labels never overlap nodes, edges, or
  each other; displaced labels get leader lines; labels carry a backing halo
  for contrast over edges.
- **Deterministic, settled layout.** Issue anchors on a radial layout (crux
  centred, others ringed by weight); positions orbit their anchor with
  enforced minimum separation. The simulation runs to settlement once, then
  freezes — no perpetual jiggle, and the layout is seeded per session so the
  same debate renders identically every visit (stable for screenshots and
  shared links).
- **Semantic zoom.** Zoomed out: issue anchors + status colour only. Mid:
  position nodes + headlines. Close: bot pills + confidence glow. The map
  never shows everything at once at any single zoom level.
- **Focus interactions.** Hover highlights the connected subgraph and dims
  the rest; click eases the camera onto the issue and opens the drawer;
  double-click background fits to canvas; replay scrubbing animates with
  eased transitions. Trackpad/touch pan-zoom supported.
- **Keyboard + accessibility** carry over from the 2026-04-19 spec: arrows
  cycle nodes, enter opens drawer, esc closes, `/` focuses search.

### Mode-generic semantics

The abstraction that keeps one map engine across modes: **anchors = the
questions at issue; nodes = attempts at them; edges = pressure applied.**

| Mode | Anchor | Node | Edge |
|---|---|---|---|
| Debate | Issue (crux central) | Position | Rebuttal / concession / frame-rejection |
| Competition | Rubric criterion | Entry (sized by score) | Critique (dissenting critiques highlighted) |
| Research | Open question in the artifact | Proposed resolution | Revision / **standing objection** |

## Part 4 — Single-model baseline comparison

Optional toggle at session creation ("Compare against single-model baseline").
When on:

1. At session start, the harness sends the topic once, unadorned, to the
   configured analysis model route and stores the answer as an `analyses` row,
   `kind='baseline'`. One extra LLM call.
2. Synthesis receives the baseline text and produces `baseline_delta`:
   considerations, counterpoints, and evidence present in the council output
   and absent from the baseline — each with a quote-verified citation to the
   transcript (reusing `extractor::verify::quote_is_substring_of`; a
   fabricated delta claim downgrades to omission, per the provenance rule —
   operational lesson 17).
3. Layer 0 renders a strip: "N considerations absent from a single-model
   answer", expandable to the list. Zero is rendered honestly as zero.

This encodes the product's value test as a per-session measurement: it shows
the sceptical technical audience what the council added, and it tells the
operator when a topic class adds nothing (persistent ~0 delta), which is
exactly the "only if genuinely more useful" criterion applied empirically.

Baseline fairness rule: the baseline prompt is the bare topic with the same
answer-length guidance as the council's R0 prompt — no handicapping, no
role-play instructions.

## Part 5 — Sessions and modes

### Data model and architecture

- `debates` table gains `session_kind TEXT NOT NULL DEFAULT 'debate'`
  (`debate | competition | research`). The table is not renamed (churn without
  benefit; API paths keep `/api/debates` with a documented alias decision
  deferred to the implementation plan for the first non-debate mode).
- Shared infrastructure is unchanged: bot dispatch (`bot_client`), extraction
  with quote verification, storage, SSE streaming, the artifact schema, and
  the Layers 0–2 UI.
- **Round logic stays bespoke per mode**: `src/orchestrator/debate/` (existing
  code relocated), `src/orchestrator/competition/`, `src/orchestrator/research/`.
  No generic round-engine DSL — the protocols' failure modes, retry ladders,
  and extraction targets are genuinely different, and a shared engine would be
  premature abstraction against house rules.

### Competition protocol

Value basis: independent parallel attempts + comparative judging + distinct
capability at critique time.

1. **Rubric at creation.** Admin defines criteria (name + description, 2–6 of
   them). Stored on the session.
2. **Blind entry round.** Each bot independently produces an entry (no peer
   context).
3. **Cross-critique round.** Each bot receives every other entry (anonymised,
   injection-framed as today) and critiques it against the rubric. This is
   where distinct capability bites — a tool-equipped bot critiques with
   research; a domain-RAG bot critiques with authority.
4. **Aggregation (harness, not a judge model).** Critiques are extracted into
   per-criterion assessments with quote-verified provenance; scores aggregate
   across critics. Self-critique is excluded. **A dissenting judge — a bot
   whose assessment materially diverges from the aggregate — is preserved in
   the artifact as a position, not averaged away** (product thesis applied to
   judging).
5. **Artifact mapping.** Criterion → `Issue`; entry-under-criterion →
   `Position` (score in `final_confidence`); critiques → edges; ranking
   summarised in `headline`/`executive_summary`.

### Research protocol

Value basis: collaborative iteration on one shared artifact with distinct
capabilities contributing; user-selected over divide-and-conquer.

1. **Seed.** Admin provides the research question and optional skeleton.
2. **Turn-based propose → critique → revise** rounds over a single shared
   draft (turn order rotates; round count configurable, default 3 cycles).
3. **Two binding integrity rules:**
   - Every revision carries provenance: bot, rationale, diff of what changed.
   - **An objection cannot be deleted by a subsequent revision.** It persists
     as a standing annotation until its author explicitly withdraws it. The
     terminal artifact is the document *plus its standing-objections layer* —
     a collaborative output that silently overwrote dissent is the same sin
     as a synthesis that manufactures consensus.
4. **Artifact mapping.** Open question → `Issue`; proposed resolution →
   `Position`; standing objection → a `Position` with `frame_rejection`
   semantics or a `Movement` with `justified` recording withdrawal; the draft
   itself stored alongside the artifact (new column on the session, decided in
   that mode's implementation plan).

## Part 6 — Session documents

Admin attaches documents (PDF/DOCX/TXT/MD) at session creation. Scale target:
bundles of hundreds of pages (user-confirmed). The bot contract is untouched —
bots receive quoted, pinned excerpts inside the prompt they already get.

### Pipeline

1. **Parse.** Council-side extraction to plain text with page/paragraph
   anchors. New `documents` table (`id`, `session_id`, `filename`, `mime`,
   `extracted_text`, `page_map JSON`, `created_at`); original bytes stored on
   EVO disk with the path on the row.
2. **Index.** Chunk (~1k chars, paragraph-aligned) and index with **BM25 via
   tantivy** — pure Rust, deterministic, no external API in the failure
   surface. Embeddings are explicitly deferred: doc-claim verification rates
   (below) give an empirical signal if lexical retrieval quality ever limits
   sessions, so the upgrade decision is evidence-based.
3. **Inject per round.** Retrieval query shaped by round focus — R0: topic;
   R2: the claims under rebuttal; R3: crux text; competition: rubric
   criterion; research: the section under revision. Top-k excerpts, each with
   a source pin (`[Bundle B, p.12, ¶3]`), injected into every bot's prompt.
   Per-prompt excerpt budget ≈ 5k chars.
4. **Frame as data.** Document text is untrusted input — a poisoned PDF is
   the same threat as a poisoned peer response. Excerpts go inside the
   existing `sanitise.rs` data-framing, never as bare prompt text.

### Doc-claim verification

Grounding discipline (user-confirmed): **bots use documents and their own
knowledge freely; claims about the documents are policed.**

Post-round, the extractor identifies each bot's claims about the documents
with their supporting quotes; each quote is substring-verified against the
document text (same machinery, and same provenance rule, as lesson 17):

- Verified → claim renders with a clickable citation pin (drawer → excerpt
  with highlight).
- Failed → never silently dropped: transcript badge on the response, plus a
  `doc_claim_failures` record per bot feeding synthesis, so misquoting the
  bundle is a visible, attributable event in the artifact ("Agent B's
  characterisation of clause 14 failed verification").

Outside knowledge remains free and unverified. Only doc-claims are policed.

### Baseline fairness extension

When a session has both documents and the baseline toggle on, the baseline
call receives the same top-k excerpts for the bare topic — otherwise the
council beats the baseline merely by having been handed the documents, and
the delta stops measuring council value.

### Privacy

Warning + operator judgement (user-confirmed for v1): the upload UI states
plainly that excerpts will be sent to every participating bot's endpoint;
roster selection remains the control. No per-bot trust flags, no redaction
machinery in v1.

### Artifact / UI touch-points

- Documents panel on the session page: filenames, page counts, parse status.
- `Position.evidence` carries doc citations with pins; issue cards show a
  "grounded" marker when evidence cites the bundle.
- Research mode is the biggest beneficiary: bundle → collaborative memo with
  standing objections is the strongest doc-bearing use case.

## Phasing

| Phase | Scope | Ships as |
|---|---|---|
| 1 | Artifact schema + synthesis prompt rewrite + resynth compatibility | One PR (backend) |
| 2 | Reading-path UI + map rework (Layers 0–2, deletions, drawer takeaway) | One or two PRs (frontend; map may split out) |
| 3 | Baseline comparison | One PR (backend + Layer 0 strip) |
| 4 | Session documents (parse, index, inject, verify) | Own implementation plan |
| 5 | Competition mode | Own implementation plan |
| 6 | Research mode | Own implementation plan |

Phases 1–2 are the committed core. 3 is small and high-value. 4 is
independently shippable against the debate mode alone. 5–6 each get a
separate plan against this design; their protocols above are design-complete
but implementation details (rubric UI, draft storage, turn scheduling) are
deferred to those plans.

## Non-goals

- Connector/transport work (lenient ingest, pull mode, OpenAI-compat) — same
  day's separate track, own spec.
- Renaming `debates` API paths or tables in Phase 1–3.
- Mobile-first design (responsive as today).
- Editing, re-running, or human participation in sessions.
- Removing the Transcript tab or any forensic capability.
- Divide-and-conquer research mode (explicitly not selected; revisit only with
  a concrete use case).
- Embedding-based retrieval, per-bot document trust flags, redaction, and
  document-level access control — all deferred; v1 is BM25 + operator
  judgement.
- A bot-pull document-search API (tool-capable bots querying the corpus
  directly) — natural extension, deferred until injected-excerpt quality is
  measured.

## Error handling

| Failure | Behaviour |
|---|---|
| MiniMax drops artifact fields | `#[serde(default)]` per field; only the missing section is empty (existing mitigation carried over) |
| Synthesis returns no issues at all | Existing empty-template salvage path; UI renders Layer 0 from `executive_summary` alone with a "synthesis incomplete" note |
| `crux_selection` row absent (legacy/failed) | No `is_crux` issue; layout centres the highest-degree issue instead |
| Baseline call fails | `baseline_delta: None`; strip not rendered; warning logged. Never blocks the session |
| Delta quote fails verification | That delta item is dropped (never shown unverified) |
| Resynthesised historical debate fails to parse | Debate keeps prior stored synthesis JSON; frontend falls back to "synthesis not available" for the Outcome tab if the shape is unreadable, Transcript unaffected |
| Competition: a bot fails its entry round | Entry marked absent; bot still critiques others (critique-only participant), noted in artifact |
| Research: objection author abstains in later rounds | Objection stands (rules say only explicit withdrawal clears it) |
| Document fails to parse (corrupt PDF, unsupported format) | Upload rejected with a per-file reason; session creation proceeds without that file only on explicit confirmation |
| Retrieval returns nothing relevant for a round | Round dispatches without excerpts; noted in round metadata, never blocks dispatch |
| Doc-claim quote fails verification | Claim flagged (transcript badge + `doc_claim_failures`), never silently dropped and never rendered as verified |

## Testing

- **Schema:** unit tests deserialising fixture MiniMax outputs — full, each
  field dropped, empty; snapshot test on the rewritten synthesis prompt.
- **Crux threading:** integration test asserting a debate with a
  `crux_selection` row yields an artifact containing exactly one
  `is_crux: true` issue.
- **Mapping semantics:** unit tests for old-concept coverage — a
  transcript that previously produced consensus + disagreement + capitulation
  + minority yields the equivalent issue-centric structure.
- **Resynth:** run `resynthesise` against fixture transcripts predating the
  crux round; assert valid artifact with no crux issue.
- **Frontend:** svelte-check + build in CI as today; component tests are not
  house practice — manual verification of Layers 0–2 against one live debate
  before ship, per the deploy checklist.
- **Baseline:** integration test with mocked model — delta items with
  verified quotes survive, fabricated-quote items dropped.
- **Documents:** fixture-based parse tests (PDF/DOCX/TXT with known page
  maps); retrieval unit tests (query → expected chunks); injection framing
  test (excerpt containing instruction-shaped text stays inert inside the
  data frame); doc-claim verification tests mirroring the extractor's
  fabricated-quote cases.
- Modes (Phases 5–6): test plans belong to their implementation plans.

## Risks

- **Schema complexity vs MiniMax reliability.** The issue structure is deeper
  than the current lists; field-dropping risk rises. Mitigation: serde
  defaults everywhere, salvage path retained, prompt requests issues one
  block at a time, and the resynth batch surfaces failures cheaply before
  users do.
- **Resynth over full history** costs one synthesis call per historical
  debate. Acceptable at current volume (Pro-tier throttle 500ms, lesson 16).
- **Map rework scope creep.** The map is the most seductive place to
  over-build. The palette, drawer pattern, and reconstruct approach are
  reused; the presentation layer is rebuilt in 2D against the quality bar
  above (retiring `ArgumentMap3D`), and graph derivation (`derive.ts`)
  retargets the issue schema. Phase 2 is the largest frontend phase and may
  split into two PRs (reading path; map) as flagged in Phasing.
- **Baseline gaming/optics.** A weak baseline flatters the council. The
  fairness rule (bare topic, same length guidance, no roles) is in the spec
  precisely so the comparison survives sceptical scrutiny.
- **PDF parsing quality.** Real bundles contain scans, tables, and exhibits
  that text extraction mangles; a bad parse silently degrades retrieval and
  doc-claim verification alike. Mitigation: parse status surfaced per file in
  the documents panel; OCR is out of scope and stated as such at upload.
- **BM25 misses semantically-relevant excerpts.** Accepted for v1; doc-claim
  verification rates and operator experience provide the evidence for an
  embeddings upgrade rather than speculation.
- **Two new modes with one live user community.** Phases 5–6 are gated behind
  the core shipping and real usage of it. Each mode needs a concrete first
  use case before its plan is written (as Sunclaw was for text-only mode):
  a named competition for Phase 5, a named research question for Phase 6.
