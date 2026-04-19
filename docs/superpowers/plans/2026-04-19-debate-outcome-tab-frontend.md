# Debate Outcome Tab — Frontend Implementation Plan

> **Phase:** PR 2 of 3 in the spec's rollout (§12). Ship the frontend
> tab and argument map **first**, using client-side reconstruction for
> round replay. PR 1 (backend per-round synthesis) becomes a follow-up.
> The UX does not change between the two PRs — just the data source for
> non-terminal round replay — so users see the feature in full tonight.
> E2E PR is PR 3.

**Goal:** Ship a new `Outcome` tab on `/debates/[id]` that renders the
debate's terminal synthesis as an interactive force-directed argument
map, with hover/click/drawer/filter interactions and a round-replay
slider powered by client-side reconstruction from the transcript.

**Architecture:** SvelteKit static site on Vercel. Svelte 5 runes for
reactivity. `d3-force` runs the physics; Svelte renders the SVG
reactively from simulation state. All work is additive — the existing
`/debates/[id]` view is extracted wholesale into `DebateTranscriptView`
and the page becomes a thin tab host.

**Tech Stack:** SvelteKit, Svelte 5 runes, TypeScript, Tailwind 4,
d3-force.

**Spec:** [docs/superpowers/specs/2026-04-19-debate-outcome-tab-design.md](../specs/2026-04-19-debate-outcome-tab-design.md)

---

## File layout

**Create:**

```
frontend/src/lib/argument-graph/
  types.ts                  # NodeKind, GraphNode, GraphEdge, GraphState
  derive.ts                 # SynthesisData → GraphState
  reconstruct.ts            # (synthesis, transcript, round) → GraphState
  simulation.ts             # d3-force wrapper, tick loop

frontend/src/lib/components/
  TabBar.svelte             # small tab control
  DebateTranscriptView.svelte  # existing /debates/[id] body extracted verbatim

frontend/src/lib/components/outcome/
  OutcomeTab.svelte         # root; loads data, owns round/filter state
  ArgumentMap.svelte        # SVG canvas, runs the simulation
  ArgumentNode.svelte       # single node + halo + label
  TensionEdge.svelte        # dashed A↔B tether
  ReplaySlider.svelte       # round slider
  OutcomeDrawer.svelte      # right-side glass drawer
  OutcomeFilters.svelte     # top-right control strip
```

**Modify:**

- `frontend/package.json` — add `d3-force@^3`
- `frontend/src/routes/debates/[id]/+page.svelte` — shrink to tab host
- `frontend/src/lib/types.ts` — no changes required v1 (reconstruction
  uses existing `TranscriptResponse`)

---

## Execution order

Tasks are grouped. Each group produces a working checkpoint.

### Group A — Plumbing (tabs + extraction)

1. Add `d3-force` to `package.json`. Run `npm install` to populate
   lockfile. Verify `npm run build` still green.
2. Create `TabBar.svelte` — pure presentation, takes `tabs: {id, label,
   disabled?}[]` + `active: string` + callback.
3. Create `DebateTranscriptView.svelte` by moving the current body of
   `+page.svelte` into it verbatim. No logic changes. Page header
   (breadcrumb, status, LIVE badge, timestamps, export button) stays
   on `+page.svelte`.
4. Modify `+page.svelte` to render header + `TabBar` + conditional
   view. Wire `?tab=` URL param to active state with
   `goto(url, { replaceState: true, noScroll: true })`. Valid tabs:
   `outcome | transcript | raw`; invalid fall back to default.
5. Default tab: `transcript` if debate is non-terminal,
   `outcome` if terminal.
6. Outcome tab content is initially a stub: "Argument map rendering…".
7. Raw tab content: `<RawJsonToggle data={synthesis ?? debate} />`.
8. Build green. Commit: "feat(frontend): tab structure on debate
   detail page".

### Group B — argument-graph library

9. Create `types.ts`:

```ts
export type NodeKind = 'topic' | 'consensus' | 'contested' | 'minority';

export interface GraphNode {
  id: string;                 // stable key: e.g. `consensus:0`, `side_a:0`
  kind: NodeKind;
  label: string;              // truncated display label
  fullText: string;           // untruncated text for drawer
  support: number;            // bots backing this node
  totalBots: number;          // denominator for "N of M"
  confidence: number | null;
  supporters: string[];       // pseudonyms
  bestArgument: string | null;
  evidence: string | null;
  disagreementIssue?: string; // for contested nodes
  sideKey?: 'a' | 'b';        // for contested nodes
  pairIndex?: number;         // links side_a and side_b of the same issue
  // Positions filled by simulation:
  x?: number;
  y?: number;
  vx?: number;
  vy?: number;
  fx?: number | null;
  fy?: number | null;
}

export interface GraphEdge {
  id: string;
  source: string;
  target: string;
  kind: 'topic-consensus' | 'topic-contested' | 'topic-minority' | 'consensus-link' | 'tension';
  dashed: boolean;
}

export interface GraphState {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export function truncate(s: string, n: number): string {
  return s.length <= n ? s : s.slice(0, n - 1) + '…';
}
```

10. Create `derive.ts`:

```ts
import type { SynthesisData, TranscriptResponse } from '$lib/types';
import type { GraphState, GraphNode, GraphEdge } from './types';
import { truncate } from './types';

export function deriveGraph(
  synthesis: SynthesisData,
  transcript: TranscriptResponse | null,
): GraphState {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];

  const totalBots = transcript?.anonymisation_log.length ?? 0;

  nodes.push({
    id: 'topic',
    kind: 'topic',
    label: 'Topic',
    fullText: synthesis.topic,
    support: totalBots,
    totalBots,
    confidence: null,
    supporters: transcript?.anonymisation_log.map(e => e.pseudonym) ?? [],
    bestArgument: null,
    evidence: null,
  });

  (synthesis.consensus_points ?? []).forEach((cp, i) => {
    const id = `consensus:${i}`;
    nodes.push({
      id,
      kind: 'consensus',
      label: truncate(cp.point ?? '', 40),
      fullText: cp.point ?? '',
      support: cp.supporting_bots?.length ?? 0,
      totalBots,
      confidence: null,
      supporters: cp.supporting_bots ?? [],
      bestArgument: null,
      evidence: cp.evidence ?? null,
    });
    edges.push({
      id: `e:topic-${id}`,
      source: 'topic',
      target: id,
      kind: 'topic-consensus',
      dashed: false,
    });
  });

  // Pairwise consensus soft-links (first few only to avoid clutter)
  const consensusIds = nodes.filter(n => n.kind === 'consensus').map(n => n.id);
  for (let i = 0; i < consensusIds.length - 1 && i < 4; i++) {
    edges.push({
      id: `e:clink-${i}`,
      source: consensusIds[i],
      target: consensusIds[i + 1],
      kind: 'consensus-link',
      dashed: false,
    });
  }

  (synthesis.live_disagreements ?? []).forEach((d, i) => {
    const aId = `side_a:${i}`;
    const bId = `side_b:${i}`;
    nodes.push({
      id: aId,
      kind: 'contested',
      label: truncate(d.side_a?.position ?? '', 40),
      fullText: d.side_a?.position ?? '',
      support: d.side_a?.bots?.length ?? 0,
      totalBots,
      confidence: null,
      supporters: d.side_a?.bots ?? [],
      bestArgument: d.side_a?.best_argument ?? null,
      evidence: null,
      disagreementIssue: d.issue,
      sideKey: 'a',
      pairIndex: i,
    });
    nodes.push({
      id: bId,
      kind: 'contested',
      label: truncate(d.side_b?.position ?? '', 40),
      fullText: d.side_b?.position ?? '',
      support: d.side_b?.bots?.length ?? 0,
      totalBots,
      confidence: null,
      supporters: d.side_b?.bots ?? [],
      bestArgument: d.side_b?.best_argument ?? null,
      evidence: null,
      disagreementIssue: d.issue,
      sideKey: 'b',
      pairIndex: i,
    });
    edges.push({ id: `e:topic-${aId}`, source: 'topic', target: aId, kind: 'topic-contested', dashed: true });
    edges.push({ id: `e:topic-${bId}`, source: 'topic', target: bId, kind: 'topic-contested', dashed: true });
    edges.push({ id: `e:tension-${i}`, source: aId, target: bId, kind: 'tension', dashed: true });
  });

  (synthesis.minority_positions ?? []).forEach((m, i) => {
    const id = `minority:${i}`;
    nodes.push({
      id,
      kind: 'minority',
      label: truncate(m.position ?? '', 40),
      fullText: m.position ?? '',
      support: 1,
      totalBots,
      confidence: m.confidence ?? null,
      supporters: m.bot ? [m.bot] : [],
      bestArgument: m.key_argument ?? null,
      evidence: null,
    });
    edges.push({ id: `e:topic-${id}`, source: 'topic', target: id, kind: 'topic-minority', dashed: true });
  });

  return { nodes, edges };
}
```

11. Create `reconstruct.ts`. For round `r`, return a new `GraphState`
    where each node's `support` / `supporters` reflect membership
    inferred from the transcript at that round. Nodes with zero
    inferred members get `support: 0` (rendered as ghost). Algorithm:

```ts
import type { SynthesisData, TranscriptResponse } from '$lib/types';
import type { GraphState } from './types';
import { deriveGraph } from './derive';

// Simple Levenshtein ratio (0..1) for string-similarity fallback.
function levRatio(a: string, b: string): number {
  if (!a || !b) return 0;
  const m = a.length, n = b.length;
  const dp = Array.from({ length: m + 1 }, (_, i) => [i, ...Array(n).fill(0)]);
  for (let j = 1; j <= n; j++) dp[0][j] = j;
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      dp[i][j] = a[i - 1] === b[j - 1]
        ? dp[i - 1][j - 1]
        : 1 + Math.min(dp[i - 1][j], dp[i][j - 1], dp[i - 1][j - 1]);
    }
  }
  return 1 - dp[m][n] / Math.max(m, n);
}

export function reconstructGraphAtRound(
  synthesis: SynthesisData,
  transcript: TranscriptResponse,
  round: number,
): GraphState {
  const base = deriveGraph(synthesis, transcript);
  if (round >= (transcript.rounds?.length ?? 0) - 1) return base;

  // Walk responses up to and including `round`, building each pseudonym's
  // last-known position summary.
  const lastPos: Record<string, string> = {};
  for (const r of transcript.rounds) {
    if (r.round_number > round) break;
    for (const resp of r.responses) {
      if (resp.abstained) continue;
      const pc = resp.position_change;
      const summary = pc?.to_summary?.trim() || resp.response.slice(0, 400);
      lastPos[resp.pseudonym] = summary;
    }
  }

  // Rebuild node support based on matches at this round.
  return {
    nodes: base.nodes.map(n => {
      if (n.kind === 'topic') return n;
      if (n.kind === 'consensus' || n.kind === 'minority') {
        // Use pseudonym membership from terminal synthesis directly —
        // at intermediate rounds, a bot is considered a supporter if
        // they were already expressing a similar position.
        const supporters = n.supporters.filter(pseudo => {
          const pos = lastPos[pseudo];
          if (!pos) return false;
          return levRatio(pos.toLowerCase(), n.fullText.toLowerCase()) > 0.2;
        });
        return { ...n, supporters, support: supporters.length };
      }
      // contested: assign pseudonym to whichever side their last position is closer to
      const supporters = n.supporters.filter(pseudo => {
        const pos = lastPos[pseudo];
        if (!pos) return false;
        return levRatio(pos.toLowerCase(), n.fullText.toLowerCase()) > 0.15;
      });
      return { ...n, supporters, support: supporters.length };
    }),
    edges: base.edges,
  };
}
```

12. Create `simulation.ts`:

```ts
import { forceSimulation, forceManyBody, forceLink, forceCollide, forceX, forceY } from 'd3-force';
import type { Simulation } from 'd3-force';
import type { GraphNode, GraphEdge } from './types';

export interface SimulationHandle {
  sim: Simulation<GraphNode, GraphEdge>;
  stop(): void;
}

export function createSimulation(
  nodes: GraphNode[],
  edges: GraphEdge[],
  width: number,
  height: number,
  onTick: () => void,
): SimulationHandle {
  const cx = width / 2;
  const cy = height / 2;

  const topic = nodes.find(n => n.kind === 'topic');
  if (topic) {
    topic.fx = cx;
    topic.fy = cy;
  }

  const sim = forceSimulation<GraphNode>(nodes)
    .force('charge', forceManyBody().strength(-180))
    .force('collide', forceCollide().radius(n => nodeRadius(n as GraphNode) + 14))
    .force('link', forceLink<GraphNode, GraphEdge>(edges)
      .id(d => d.id)
      .distance(e => linkDistance(e))
      .strength(0.35))
    .force('attract-consensus', forceX<GraphNode>(cx + 140)
      .strength(n => n.kind === 'consensus' ? 0.05 : 0)
    )
    .force('attract-consensus-y', forceY<GraphNode>(cy - 60)
      .strength(n => n.kind === 'consensus' ? 0.05 : 0)
    )
    .force('attract-minority', forceX<GraphNode>(cx - 280)
      .strength(n => n.kind === 'minority' ? 0.06 : 0)
    )
    .force('attract-contested-a', forceX<GraphNode>(cx - 180)
      .strength(n => n.kind === 'contested' && n.sideKey === 'a' ? 0.07 : 0)
    )
    .force('attract-contested-b', forceX<GraphNode>(cx + 220)
      .strength(n => n.kind === 'contested' && n.sideKey === 'b' ? 0.07 : 0)
    )
    .alphaMin(0.01)
    .alphaDecay(0.04);

  sim.on('tick', onTick);
  return {
    sim,
    stop: () => sim.stop(),
  };
}

export function nodeRadius(n: GraphNode): number {
  const base = n.kind === 'topic' ? 28 : 10;
  const boost = Math.min(n.support * 2.5, 14);
  return base + boost;
}

function linkDistance(e: GraphEdge): number {
  switch (e.kind) {
    case 'topic-consensus': return 120;
    case 'topic-contested': return 180;
    case 'topic-minority': return 230;
    case 'consensus-link': return 90;
    case 'tension': return 260;
  }
}
```

### Group C — Components

13. `ArgumentNode.svelte` — renders one node. Props: `node: GraphNode`,
    `selected: boolean`, `ghost: boolean`, `onClick: (id: string) => void`,
    `onHover: (id: string | null) => void`. Uses SVG `<g>` with inner
    `<circle>`, outer blurred halo circle, two `<text>` labels (label
    above, "N of M" below). Colour by `kind`:
    - topic: neutral silver gradient
    - consensus: `#10b981`
    - contested: `#f43f5e`
    - minority: `#8b5cf6`

    Ghost state: fill → transparent, stroke → low-opacity, no label.

14. `TensionEdge.svelte` — renders a bezier between two nodes. Props:
    `sourceX, sourceY, targetX, targetY, kind, dashed`. Uses single
    `<path>` with quadratic control point halfway between source and
    target perpendicular to the line.

15. `ArgumentMap.svelte`:

    - Props: `graph: GraphState`, `selectedId: string | null`,
      `onNodeClick`, `onEdgeClick`, `hiddenKinds: Set<NodeKind>`,
      `highlightedSupporters: string[]` (for filters).
    - Owns a `$state` canvas size (ResizeObserver).
    - Creates simulation on mount or whenever the graph identity
      changes. Re-seeds node positions from the previous stable layout
      (by id) if available.
    - Renders edges (behind), then nodes. Uses `$derived` for
      filtered views.
    - `<svg viewBox>` scales to container width; min-height 380px,
      auto-grow to 520px if nodes > 20.
    - `role="img"` with `aria-label="Argument map: N consensus points,
      M disagreements, K minority positions"`.

16. `OutcomeDrawer.svelte` — right-side glass panel (fixed position,
    320px wide, slides in). Props: `node: GraphNode | null`,
    `disagreement: { issue: string; sideA: GraphNode; sideB: GraphNode } | null`
    (populated for tension-edge selection), `onClose`.
    - Renders claim text, `N of M bots` pill, confidence (if any),
      best argument, evidence, supporter pseudonyms as pills.
    - For tension: side-by-side columns.
    - Focus trap while open; Esc closes; returns focus to trigger.

17. `ReplaySlider.svelte`:

    - Props: `rounds: number` (total, derived from transcript),
      `round: number` (current, `-1` = Final), `onChange`,
      `playing: boolean`, `onPlayToggle`.
    - Track with N+1 ticks (`R0 · R1 · … · Final`); Final default.
    - Click/drag ticks. Play auto-advances `1.5s` per tick. Stops on
      Final.

18. `OutcomeFilters.svelte`:

    - Props: `hiddenKinds: Set<NodeKind>`, `supporters: string[]`,
      `highlightedSupporter: string | null`, callbacks.
    - Three toggles (hide minority, hide contested, show consensus
      only), one pseudonym picker.

19. `OutcomeTab.svelte` (root):

    - Props: `debate: DebateResponse`, `synthesis: SynthesisResponse | null`,
      `transcript: TranscriptResponse | null`.
    - If no terminal synthesis → empty-state card.
    - Owns: `selectedRound` (-1 default = Final), `selectedNodeId`,
      `selectedEdgeId`, `hiddenKinds`, `highlightedSupporter`,
      `playing`.
    - Derives `graph` from `selectedRound`: for `-1` uses `deriveGraph`,
      otherwise uses `reconstructGraphAtRound`. Shows `Inferred`
      badge for replayed rounds.
    - Composes: `<ArgumentMap /> <OutcomeFilters /> <ReplaySlider />
      <OutcomeDrawer />`.

### Group D — Tab wiring

20. Modify `+page.svelte`:

```svelte
<script lang="ts">
  // ... existing imports ...
  import TabBar from '$lib/components/TabBar.svelte';
  import DebateTranscriptView from '$lib/components/DebateTranscriptView.svelte';
  import OutcomeTab from '$lib/components/outcome/OutcomeTab.svelte';
  import RawJsonToggle from '$lib/components/RawJsonToggle.svelte';
  import { page } from '$app/state';
  import { goto } from '$app/navigation';

  type Tab = 'outcome' | 'transcript' | 'raw';
  const TERMINAL = ['complete', 'cancelled', 'failed'];

  let activeTab = $derived<Tab>((() => {
    const p = page.url.searchParams.get('tab') as Tab | null;
    if (p === 'outcome' || p === 'transcript' || p === 'raw') return p;
    return debate && TERMINAL.includes(debate.status) ? 'outcome' : 'transcript';
  })());

  function setTab(t: Tab) {
    const url = new URL(page.url);
    url.searchParams.set('tab', t);
    goto(url, { replaceState: true, noScroll: true, keepFocus: true });
  }

  const tabs = $derived([
    { id: 'outcome', label: 'Outcome', disabled: !debate || !TERMINAL.includes(debate.status) },
    { id: 'transcript', label: 'Transcript' },
    { id: 'raw', label: 'Raw' },
  ]);
</script>

<!-- header stays (breadcrumb, status, LIVE, title, meta) -->

<TabBar {tabs} active={activeTab} onChange={setTab} />

{#if activeTab === 'outcome'}
  <OutcomeTab {debate} {synthesis} {transcript} />
{:else if activeTab === 'raw'}
  <RawJsonToggle data={synthesis ?? debate} />
{:else}
  <DebateTranscriptView {debate} {synthesis} {transcript} {sseConnected} />
{/if}
```

### Group E — Verify

21. `cd frontend && npm install && npm run build`. Fix any type or
    build errors. Do not disable strict checks.
22. `cd frontend && npx svelte-check` — zero errors.
23. Local dev smoke: `npm run dev`, open a terminal debate in the
    browser, confirm:
    - Outcome tab default
    - Map renders, nodes clickable, drawer populates
    - Replay slider rewinds graph, ghost nodes visible, Inferred badge
      shown on non-Final
    - Filters hide/show the correct kinds
    - `?tab=transcript` round-trips
    - Non-terminal debate shows transcript tab default; outcome tab
      disabled
24. Commit per group.

### Group F — Deploy

25. Push branch, open PR to `main`. Unified release gate:
    - EVO backend tests green (`./scripts/sync-evo.sh` — no backend
      changes here, but keep the habit)
    - `npm run build` green
    - `./scripts/check-auth-provider.sh` green
26. Merge PR once Vercel preview is green.
27. Vercel auto-deploys to production. Manually verify
    `https://lqcouncil.com/debates/<some-terminal-id>?tab=outcome`
    renders correctly.

---

## Scope notes

- **Per-round synthesis is OUT of this PR.** Replay uses
  `reconstruct.ts` with an `Inferred` badge overlay. A follow-up PR
  will add the `synthesis_rounds` backend and SSE, at which point the
  frontend detects per-round data is available and switches source
  automatically. `OutcomeTab` already has the hook: if a
  `roundSyntheses: SynthesisData[]` prop arrives, use it instead of
  `reconstructGraphAtRound`.
- **d3-force typed imports.** `@types/d3-force` is bundled inside the
  `d3-force` v3 package, so no extra `@types` dependency.
- **Chart.js is already in the tree** (for `ConfidenceChart`). Do not
  introduce additional chart libs.
- **Bot names stay hidden.** Drawer uses pseudonyms only, same as the
  rest of the app.
