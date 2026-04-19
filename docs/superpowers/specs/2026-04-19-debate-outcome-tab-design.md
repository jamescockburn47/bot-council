# Debate Outcome Tab — Design

**Status:** Design approved, implementation plan pending.
**Date:** 2026-04-19
**Owner:** jamescockburn47
**Audience:** blended end-reader, argument-first (not bot-behaviour-first).

## 1. Problem

The current `/debates/[id]` page is a dense forensic scroll: synthesis
cards, confidence chart, round accordion, divergence panel, citation
badge, anonymisation log, markdown export. It works for stress-testing
a debate but is not *visual*, and it privileges bot behaviour (who said
what, who shifted) over the arguments themselves. A reader who just
wants to see the *shape* of the council's conclusion — where the common
ground is, where the live disagreements are, who holds the minority
line — has to reconstruct it mentally from cards and text.

We want a dedicated **Outcome** tab that renders the debate's end state
as an interactive, argument-first visualization. It must look clear,
modern, and expensive, and it must let the user replay how the argument
landscape evolved round by round.

## 2. Goals / non-goals

### Goals

1. Add an Outcome tab on `/debates/[id]` that renders an interactive
   argument map. Argument-first; bot pseudonyms are secondary signal.
2. Visual register: dark, precise, editorial. No Canvas pixel-art. SVG
   with careful typography, subtle glow, soft animation.
3. Interactive: hover → tooltip, click → drawer, round-replay slider,
   filters, zoom/pan, search, keyboard navigation.
4. Round replay uses authoritative per-round synthesis (new backend
   work), with a client-side reconstruction fallback when a round's
   synthesis is missing or failed.
5. Purely additive: existing scroll view (now under a Transcript tab)
   is untouched.

### Non-goals

- No editing, re-running, or argument threading beyond what the
  synthesiser already produces.
- No bot-centric analytics here. That stays on the Transcript tab.
- No mobile-first design. Responsive but desktop-optimised.
- No live updates during a running debate in v1. Outcome tab is
  available only once the debate reaches a terminal state.

## 3. Information architecture

Tab bar sits immediately below the debate header:

```
[ Outcome ]  Transcript   Raw
```

- **Outcome** — new argument map (this spec). Visible only when the
  debate is terminal; otherwise renders a skeletal placeholder.
- **Transcript** — the full existing scroll view, unchanged.
- **Raw** — the existing `<RawJsonToggle>` content, extracted for
  cleanliness.

Default tab: **Outcome** for terminal debates, **Transcript** for
in-progress debates. Tab state is URL-synced via
`?tab=outcome|transcript|raw` (back-button, refresh, deep-links all
preserve the view). Invalid `?tab=` values fall back to the default.

Rationale for an additive tab vs a full restructure: preserves the
working forensic surface, makes the new view droppable without
collateral damage.

## 4. Visualization model

### 4.1 Nodes

Three node types, all derived from `SynthesisData`:

| Node type | Source | Label | Size encoding | Colour |
|---|---|---|---|---|
| Topic | `debate.topic` | "Topic" + short debate id | fixed ≈ 32 px | neutral silver gradient |
| Consensus | `consensus_points[i]` | `point` (truncated to 40 chars) | `supporting_bots.length` | emerald `#10b981` |
| Contested | `live_disagreements[i].side_a` and `.side_b` | `position` (truncated) | bots on that side | rose `#f43f5e` |
| Minority | `minority_positions[i]` | `position` (truncated) | `confidence` | violet `#8b5cf6` |

Nodes have a soft outer glow (SVG `feGaussianBlur` halo) and an inner
radial gradient. Labels above the node.

### 4.2 Edges

- **Topic → every position node**, always. Colour matches the node;
  consensus edges solid, contested/minority dashed.
- **Side A ↔ Side B tension tether** for each disagreement — dashed
  rose, thinner, curved. The visual pair.
- **Same-cluster soft link** between consensus nodes (subtle
  `#10b98133`), to suggest the group.

No bot-to-node edges. Pseudonyms surface in the drawer, tooltips, and
the optional "highlight supporters" filter.

### 4.3 Clustering — the "organic" feel

Lightweight force simulation (d3-force) produces positions:

- Topic is a fixed anchor at canvas centre.
- Consensus nodes share an invisible attractor ("common ground") near
  the topic — pulls them into a cluster.
- Each disagreement has a paired attractor ≈ 280 px apart; side A binds
  to one, side B to the other.
- Minority nodes have no shared attractor — they settle at the
  periphery.
- Standard charge repulsion between all nodes; small collision radius.

Visual reference: "V2 Organic graph" mockup agreed during the
brainstorming session (see the committed HTML under
`.superpowers/brainstorm/` if still available locally — directory is
gitignored and ephemeral; the implementation should re-create the same
look from this spec alone).

### 4.4 Round replay — data source

The debate stores per-round synthesis in a new `synthesis_rounds`
table (§6.1). At render, the Outcome tab fetches the full round series
plus the terminal synthesis and switches the active graph when the
user moves the slider.

**Fallback.** For rounds whose synthesis has failed or not yet
completed, we degrade gracefully via `reconstruct.ts`:

1. Take the terminal synthesis clusters as ground truth.
2. For each pseudonym, walk their transcript responses round-by-round
   and assign their position at round `r` to the terminal cluster it
   most resembles (consensus/minority: pseudonym membership;
   disagreement: Levenshtein-ratio match of `position_change.to_summary`
   against `side_a.position` vs `side_b.position`).
3. Nodes with zero inferred members at round `r` render as ghost nodes
   (dimmed, no label).
4. An `inferred` badge overlays the top-right of the canvas so the
   reader knows this round is a best-effort inference, not an LLM pass.

The drawer always shows actual transcript text at the selected round
for the selected cluster — that is the authoritative source the
reader can fall back to.

## 5. Interaction model

### 5.1 Primary

1. **Hover node** → tooltip: full claim text, support count ("3 of 5"),
   mean confidence, citation validity (if any).
2. **Click node** → right-side glass drawer:
   - Claim / position text
   - Best argument (from synthesis)
   - Evidence quotes with citation pins
   - Supporting pseudonyms as pills
   - "Jump to in transcript" button → switches tabs, expands the
     relevant round, highlights the responses.
3. **Click topic** → drawer shows debate meta (goal mode, round count,
   total tokens, participant roster, citation-check summary, and a
   one-line "council verdict" field — small new addition to the
   synthesis schema, see §6.1).
4. **Click tension tether** → drawer shows a side-by-side compare of
   both positions, best arguments, counter-evidence.

### 5.2 Replay slider

Bottom-of-canvas pill track: `R0 · R1 · R2 · R3 · R4 · Final`. Default
tick = `Final`.

- Click / drag a tick → rebuild graph from per-round synthesis (or
  fallback).
- `Play` → auto-advance one round per ≈ 1.5 s with animated node
  transitions.
- Nodes that didn't exist at round `r` render as ghost nodes (opacity
  ≈ 0.2, no label).
- Nodes that grew / shrank animate size change via spring easing.
- Per-round loading states: spinner on the tick while that round's
  synthesis is running; amber dot on the tick if it failed.

### 5.3 Filters & navigation

Top-right control strip:

- Toggle: **Hide minority**
- Toggle: **Hide contested** (consensus-only view)
- **Supporters filter** — pick a pseudonym; every node they back stays
  lit, the rest fade to 20 %.

Also:

- Zoom & pan (wheel / pinch / cmd-drag); double-click fits to canvas.
- **Search** — cmd/ctrl+K opens fuzzy search over claim text; enter
  highlights and zooms.
- **Keyboard**: arrows cycle nodes in insertion order, enter opens
  drawer, esc closes, `/` focuses search.

### 5.4 Non-interactions

- No editing; no re-running; no argument threading.
- No bot-centric views.
- Mobile: drawer stacks below canvas; interactions degrade gracefully
  but are not the target platform.

## 6. Implementation

### 6.1 Backend (Rust / Axum / SQLite)

**Migration** `20260419000002_add_per_round_synthesis.sql`:

```sql
CREATE TABLE synthesis_rounds (
    debate_id TEXT NOT NULL REFERENCES debates(id) ON DELETE CASCADE,
    round_number INTEGER NOT NULL,
    synthesis_json TEXT NOT NULL,
    model_used TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending','running','complete','failed')),
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (debate_id, round_number)
);
CREATE INDEX idx_synthesis_rounds_debate ON synthesis_rounds(debate_id);
```

The existing `synthesis` table keeps the terminal result for
backwards-compatibility.

**Synthesis schema addition.** `SynthesisData` gains a single optional
field:

```rust
pub struct SynthesisData {
    // ... existing fields ...
    pub council_verdict: Option<String>,  // one-line summary for topic drawer
}
```

The synthesis prompt (`src/synthesiser/mod.rs::build_synthesis_prompt`)
is extended with one paragraph instructing the model to emit a
single-sentence verdict field in the output JSON, no more than 140
characters, stating the council's net position in plain English.
`serde(default)` keeps deserialisation of older records working so
historical debates load cleanly.

**Orchestrator** (`src/orchestrator/`):

After the existing terminal synthesis call succeeds, spawn a Tokio
task that synthesises rounds `1..N-1` **serially** (local Gemma ≈ 31B
model is GPU-bound; concurrent 31B calls would thrash memory).

Each per-round call reuses `synthesiser::run_synthesis` with a small
prompt variant:

> "Synthesise based on the transcript through round {N} only. Do not
> anticipate subsequent rounds. The debate is ongoing from your
> perspective."

Round failures are persisted with `status = failed` and an
`error_message`; subsequent rounds continue.

**API** (`src/api/synthesis.rs`):

- `GET /debates/{id}/synthesis/rounds` → `{ rounds: [{ round_number,
  synthesis, status, error_message? }] }`, ordered by `round_number`.
  Terminal round is included.
- `GET /debates/{id}/synthesis` — unchanged (terminal only).
- Auth: `RequireAuth` (same as terminal synthesis).

**SSE** (`src/api/events.rs`):

- New event on the existing `/debates/{id}/stream` channel:
  `synthesis:round_completed { round_number, synthesis }`. Fires per
  round after insert, after `debate:completed`.

### 6.2 Frontend (SvelteKit + Svelte 5 runes)

**Library:** `d3-force` (≈ 12 KB gzipped). Uses only `forceSimulation`,
`forceManyBody`, `forceLink`, a custom attractor force.

**File layout**:

```
frontend/src/lib/components/outcome/
  OutcomeTab.svelte       # root; loads data, owns round/filter state
  ArgumentMap.svelte      # SVG canvas, runs the simulation
  ArgumentNode.svelte     # single node with halo + label
  TensionEdge.svelte      # dashed A↔B tether
  ReplaySlider.svelte     # round slider with per-round status indicators
  OutcomeDrawer.svelte    # right-side glass drawer
  OutcomeFilters.svelte   # top-right control strip

frontend/src/lib/argument-graph/
  types.ts                # NodeKind, GraphNode, GraphEdge, GraphState
  derive.ts               # synthesis → nodes + edges
  reconstruct.ts          # client-side per-round fallback
  simulation.ts           # d3-force wrapper, reactive store
```

Each file stays under 300 lines (CLAUDE.md binding rule).

**Tab plumbing** (`frontend/src/routes/debates/[id]/+page.svelte`):

- Extract the current body into `DebateTranscriptView.svelte`
  verbatim (no logic changes).
- Add tab bar as a small inline control.
- Tab state via `goto('?tab=outcome', { replaceState: true, noScroll:
  true })`.

**Data flow**

1. On mount: fetch `/debates/{id}/synthesis` and
   `/debates/{id}/synthesis/rounds` in parallel.
2. `derive.ts` builds `GraphState { nodes, edges }` for the selected
   round.
3. `simulation.ts` runs d3-force with custom attractors; exposes
   position via a Svelte `$state`-backed store that ticks until
   `α < 0.01`.
4. `ArgumentMap.svelte` renders nodes/edges reactively via `$derived`.
5. SSE subscription stays for live debates; `synthesis:round_completed`
   events upsert into the rounds store and mark the slider tick ready.

### 6.3 Accessibility

- Map has `role="img"` with an `aria-label` summary
  ("Argument map: N consensus points, M disagreements, K minority
  positions").
- Full data is duplicated in the drawer and the Transcript tab, both
  keyboard-navigable.
- Respects `prefers-reduced-motion`: no settle animation, no replay
  auto-play, no spring transitions.
- Colour is not the only channel: node shape + position + size also
  encode the category.

## 7. Edge cases & empty states

| State | Behaviour |
|---|---|
| Debate still running | Placeholder: "Argument map renders once the debate completes." Transcript tab is default. |
| Terminal synthesis present, rounds pending | Final graph renders immediately. Slider `Final` unlocked; earlier ticks show spinners and are disabled with tooltip "Synthesising round N…". |
| Per-round synthesis failed | Tick shows amber dot. Clicking it falls back to client reconstruction with an `inferred` badge on the canvas. |
| Terminal synthesis failed | Error card with retry; no force sim runs. |
| Debate cancelled / failed | Outcome tab hidden from the bar entirely; `?tab=outcome` falls back to Transcript. |
| Zero consensus points | No common-ground attractor. Map still renders. |
| Zero live disagreements | No tension tethers. Normal. |
| Single-participant debate | Single consensus node per claim, support 1/1. Sparse by design. |
| Dense debate (> 20 nodes) | Node sizes shrink 30 %, labels truncate to 40 chars, canvas min-height grows to 520 px. |
| Duplicate claim text across categories | Treated as distinct nodes; no dedup. Synthesiser's job to avoid. |

## 8. Performance

- d3-force tick budget: target ≤ 200 ms to settle for a typical
  12-node graph. Hard stop at 2 s. Simulation ends at `α < 0.01`.
- Svelte reactivity reconciles ≤ 20 nodes/tick in under 2 ms. No
  Canvas fallback needed at this scale.
- Round switch re-seeds positions from the previous stable layout (not
  from zero). Re-run simulation for ≤ 500 ms.
- Payload: 5 rounds × ≈ 3 KB each ≈ 15 KB per debate. Fetched once per
  tab session.
- Labels: each label is one `<text>` element; we cap label length.

## 9. Security / privacy

- No new auth surface; `/synthesis/rounds` uses `RequireAuth`.
- Pseudonyms only in drawer; real bot names remain admin-only (existing
  boundary preserved).
- All claim text rendered via SVG `<text>` content; no `innerHTML`.
  User-controlled strings cannot execute HTML.

## 10. Observability

- Tracing spans: `synthesis.per_round` with structured `round_number`
  field on each background task.
- Sentry: per-round failures tagged with `debate_id` and
  `round_number`.
- Frontend error boundary catches force-sim or render crashes and
  falls back to "Could not render argument map; see Transcript tab".

## 11. Testing

### 11.1 Backend

Integration tests (`tests/synthesis_rounds_*.rs`, `tower::ServiceExt`
+ in-memory SQLite, `common::admin_auth`):

- `GET /synthesis/rounds` — empty when none exist; correct ordering;
  404 for missing debate; 401 without auth.
- Per-row insert is idempotent on `(debate_id, round_number)`.
- Orchestrator test: mock bot clients + mock synthesiser, run a
  3-round debate, assert 3 rows appear after terminal settles, each
  `status = complete`.
- SSE test: subscribe to `/stream`, complete debate, assert
  `synthesis:round_completed` events arrive for rounds `1..N-1` after
  `debate:completed`.
- Failure path: force one per-round call to error; assert row persists
  with `status = failed`, subsequent rounds continue, SSE carries
  failure.

### 11.2 Frontend

Vitest units for `argument-graph/`:

- `derive.ts`: canned `SynthesisData` fixture → expected node+edge
  list; test each node type and empty permutations.
- `reconstruct.ts`: known capitulation fixture → assigns pseudonym
  to correct side across rounds.
- `simulation.ts`: deterministic seed + 30 ticks → consensus nodes
  within radius X of topic, minority beyond radius Y.

Component tests (Vitest + `@testing-library/svelte`):

- `OutcomeTab`: placeholder when non-terminal, map when terminal,
  drawer on click.
- `ReplaySlider`: disabled ticks for pending rounds; selection fires
  `onRoundChange`.
- `OutcomeFilters`: each toggle hides/shows expected node class.

Accessibility: `axe-core` assertions — `role="img"` with meaningful
label, interactive controls keyboard-reachable, drawer traps focus
when open.

### 11.3 E2E (deferred)

One Playwright smoke test in a follow-up PR: load a seeded debate,
Outcome tab renders a map with ≥ 1 node, clicking a node opens the
drawer with expected claim text. Not in the first PR.

### 11.4 Manual verification (per CLAUDE.md binding rule #10)

1. `./scripts/sync-evo.sh` green, including per-round synthesis tests.
2. `cd frontend && npm run build` green, no TS or Svelte warnings.
3. `bash ./scripts/check-auth-provider.sh` green.
4. Deploy to EVO + Vercel. On a real debate:
   - Outcome tab appears after terminal state.
   - Map renders, settles, nodes clickable, drawer populates.
   - Slider ticks light up as rounds arrive via SSE.
   - A tiny debate (2 bots, 1 round) still renders sanely.
5. Mobile viewport (≤ 480 px): drawer stacks, everything readable.

## 12. Rollout

Single PR per logical change (per CLAUDE.md):

1. **Backend PR** — migration, `SynthesisData.council_verdict` field,
   per-round orchestrator task, `GET /synthesis/rounds`, new SSE
   event, tests.
2. **Frontend PR** — tab structure, extracted `DebateTranscriptView`,
   outcome components, `argument-graph/` lib, d3-force dependency,
   tests.
3. **E2E smoke test PR** (follow-up).

Each PR must pass the Unified Release Gate (backend tests, frontend
build, auth provider check, deploy, health). Full binding rules in
CLAUDE.md.

## 13. Open questions

- Animation spring physics — do we use svelte/motion `spring`, or
  hand-rolled? Defer to implementation.
- Whether to precompute node positions server-side for sharing a
  stable link-to-map screenshot. Not needed v1; revisit if users
  screenshot the map routinely.
- If a future UI framework change replaces d3-force, swap behind the
  `simulation.ts` interface — consumers shouldn't notice.
