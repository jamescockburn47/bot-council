<script lang="ts">
  import { untrack } from 'svelte';
  import type { GraphEdge, GraphNode, GraphState, NodeKind } from '$lib/argument-graph/types';
  import { createSimulation, type SimulationHandle } from '$lib/argument-graph/simulation';
  import ArgumentNode from './ArgumentNode.svelte';

  let {
    graph,
    selectedNodeId = null,
    hiddenKinds = new Set(),
    highlightedSupporter = null,
    onNodeClick,
    onEdgeClick,
  }: {
    graph: GraphState;
    selectedNodeId?: string | null;
    hiddenKinds?: Set<NodeKind>;
    highlightedSupporter?: string | null;
    onNodeClick: (id: string) => void;
    onEdgeClick: (id: string) => void;
  } = $props();

  let container: HTMLDivElement | undefined = $state();
  let width = $state(960);
  let height = $state(420);

  // A simple tick counter that simulation bumps on every frame; SVG template
  // reads it to force a re-render since d3-force mutates node positions in
  // place.
  let tick = $state(0);

  let handle: SimulationHandle | null = null;
  let lastGraphId = '';

  // Preserve positions across graph identity changes so round switches don't
  // teleport nodes. Key: node.id → { x, y }.
  const positionCache = new Map<string, { x: number; y: number }>();

  function seedFromCache(nodes: GraphNode[]) {
    const cx = width / 2;
    const cy = height / 2;
    for (const n of nodes) {
      const cached = positionCache.get(n.id);
      if (cached) {
        n.x = cached.x;
        n.y = cached.y;
      } else {
        // Spawn near centre with small jitter.
        n.x = cx + (Math.random() - 0.5) * 60;
        n.y = cy + (Math.random() - 0.5) * 60;
      }
    }
  }

  function stashPositions(nodes: GraphNode[]) {
    for (const n of nodes) {
      if (typeof n.x === 'number' && typeof n.y === 'number') {
        positionCache.set(n.id, { x: n.x, y: n.y });
      }
    }
  }

  function rebuild() {
    handle?.stop();
    if (!graph.nodes.length) return;
    seedFromCache(graph.nodes);
    handle = createSimulation(graph.nodes, graph.edges, width, height, () => {
      tick++;
    });
    // Converge synchronously before the first render so nodes don't show up
    // stacked at the origin while the async d3-force rAF loop warms up.
    // `sim.tick(n)` advances alpha n times without dispatching the tick
    // event, so we bump `tick` manually once to trigger Svelte's reactivity.
    handle.sim.tick(300);
    tick++;
  }

  $effect(() => {
    // Rebuild the simulation when the graph identity or size changes.
    const signature = `${graph.nodes.map((n) => n.id).join(',')}|${width}x${height}`;
    if (signature === lastGraphId) return;
    lastGraphId = signature;
    untrack(rebuild);
  });

  // Persist positions whenever a tick fires so the cache is warm for next
  // round switch.
  $effect(() => {
    tick;
    untrack(() => stashPositions(graph.nodes));
  });

  // Watch container size.
  $effect(() => {
    if (!container) return;
    const el = container;
    const ro = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const w = Math.max(600, entry.contentRect.width);
        const h = Math.max(420, Math.min(640, entry.contentRect.width * 0.55));
        if (Math.abs(w - width) > 4 || Math.abs(h - height) > 4) {
          width = w;
          height = h;
        }
      }
    });
    ro.observe(el);
    return () => ro.disconnect();
  });

  // Effect-return cleanup rather than onDestroy. The compiled onDestroy helper
  // in the shipped bundle resolves to the SSR-context path (Mt.r.on_destroy on
  // CkH79pZK's server-renderer context) which is null during CSR hydration,
  // producing "Cannot read properties of null (reading 'r')". $effect return
  // cleanup goes through the CSR runtime unambiguously.
  $effect(() => () => handle?.stop());

  // ---- Derived rendering helpers (read `tick` to update with sim) ----

  let renderedNodes = $derived.by(() => {
    // Touch `tick` so this derivation re-runs on every simulation frame.
    // Snapshot positions into plain primitives on new objects per tick so the
    // child component receives fresh prop values — reading `node.x` directly
    // inside the child doesn't trigger Svelte reactivity (d3-force mutates
    // plain properties, which aren't tracked).
    tick;
    return graph.nodes
      .filter((n) => n.kind === 'topic' || !hiddenKinds.has(n.kind))
      .map((n) => ({ id: n.id, node: n, x: n.x ?? 0, y: n.y ?? 0 }));
  });

  let renderedEdges = $derived.by(() => {
    tick;
    const nodeById = new Map(graph.nodes.map((n) => [n.id, n] as const));
    return graph.edges
      .map((e) => {
        const s = typeof e.source === 'string' ? nodeById.get(e.source) : (e.source as GraphNode);
        const t = typeof e.target === 'string' ? nodeById.get(e.target) : (e.target as GraphNode);
        if (!s || !t) return null;
        // Drop edges that point into hidden kinds.
        if (
          (s.kind !== 'topic' && hiddenKinds.has(s.kind)) ||
          (t.kind !== 'topic' && hiddenKinds.has(t.kind))
        ) {
          return null;
        }
        return {
          edge: e,
          sx: s.x ?? 0,
          sy: s.y ?? 0,
          tx: t.x ?? 0,
          ty: t.y ?? 0,
        };
      })
      .filter((v): v is Exclude<typeof v, null> => v !== null);
  });

  function edgeStroke(e: GraphEdge): string {
    switch (e.kind) {
      case 'topic-consensus':
      case 'consensus-link':
        return 'rgba(16,185,129,0.55)';
      case 'topic-contested':
      case 'tension':
        return 'rgba(244,63,94,0.6)';
      case 'topic-minority':
        return 'rgba(139,92,246,0.55)';
    }
  }

  function bezier(sx: number, sy: number, tx: number, ty: number): string {
    const mx = (sx + tx) / 2;
    const my = (sy + ty) / 2;
    // Perpendicular offset for curvature; keeps lines from overlapping.
    const dx = tx - sx;
    const dy = ty - sy;
    const len = Math.hypot(dx, dy) || 1;
    const nx = -dy / len;
    const ny = dx / len;
    const offset = Math.min(40, len * 0.15);
    const cx = mx + nx * offset;
    const cy = my + ny * offset;
    return `M ${sx} ${sy} Q ${cx} ${cy} ${tx} ${ty}`;
  }

  let ariaLabel = $derived.by(() => {
    const c = graph.nodes.filter((n) => n.kind === 'consensus').length;
    // Half of contested nodes = number of disagreement issues.
    const d = graph.nodes.filter((n) => n.kind === 'contested').length / 2;
    const m = graph.nodes.filter((n) => n.kind === 'minority').length;
    return `Argument map: ${c} consensus points, ${d} disagreements, ${m} minority positions.`;
  });

  function isHighlighted(n: GraphNode): boolean {
    if (!highlightedSupporter) return false;
    return n.supporters.includes(highlightedSupporter);
  }

  function isDimmed(n: GraphNode): boolean {
    if (!highlightedSupporter) return false;
    if (n.kind === 'topic') return false;
    return !n.supporters.includes(highlightedSupporter);
  }
</script>

<div
  bind:this={container}
  class="relative w-full rounded-xl border border-[var(--border)] bg-[#0b0b11] overflow-hidden"
  style="min-height: 420px;"
>
  <svg
    role="img"
    aria-label={ariaLabel}
    width={width}
    height={height}
    viewBox="0 0 {width} {height}"
    style="display: block; width: 100%; height: {height}px;"
  >
    <defs>
      <radialGradient id="am-topic-grad" cx="50%" cy="50%" r="50%">
        <stop offset="0%" stop-color="#ffffff" stop-opacity="0.95" />
        <stop offset="55%" stop-color="#d4d4dc" stop-opacity="0.65" />
        <stop offset="100%" stop-color="#1a1a22" stop-opacity="0.9" />
      </radialGradient>
      <filter id="am-halo" x="-100%" y="-100%" width="300%" height="300%">
        <feGaussianBlur stdDeviation="10" />
      </filter>
    </defs>

    <!-- Background subtle spotlight -->
    <defs>
      <radialGradient id="am-bg-spot" cx="50%" cy="40%" r="60%">
        <stop offset="0%" stop-color="#60a5fa" stop-opacity="0.05" />
        <stop offset="100%" stop-color="#60a5fa" stop-opacity="0" />
      </radialGradient>
    </defs>
    <rect x="0" y="0" width={width} height={height} fill="url(#am-bg-spot)" />

    <!-- Edges first (behind nodes) -->
    <g>
      {#each renderedEdges as re (re.edge.id)}
        <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
        <path
          d={bezier(re.sx, re.sy, re.tx, re.ty)}
          stroke={edgeStroke(re.edge)}
          stroke-width={re.edge.kind === 'tension' ? 1.4 : 1.3}
          fill="none"
          stroke-dasharray={re.edge.dashed ? '4 4' : ''}
          stroke-linecap="round"
          style="cursor: {re.edge.kind === 'tension' ? 'pointer' : 'default'};"
          onclick={(ev) => {
            if (re.edge.kind === 'tension') {
              ev.stopPropagation();
              onEdgeClick(re.edge.id);
            }
          }}
          role={re.edge.kind === 'tension' ? 'button' : 'presentation'}
          tabindex={re.edge.kind === 'tension' ? 0 : -1}
          aria-label={re.edge.kind === 'tension' ? 'Disagreement tether — click to compare sides' : undefined}
          onkeydown={(ev) => {
            if (re.edge.kind === 'tension' && (ev.key === 'Enter' || ev.key === ' ')) {
              ev.preventDefault();
              onEdgeClick(re.edge.id);
            }
          }}
        />
      {/each}
    </g>

    <!-- Nodes -->
    <g>
      {#each renderedNodes as rn (rn.id)}
        <ArgumentNode
          node={rn.node}
          x={rn.x}
          y={rn.y}
          selected={selectedNodeId === rn.id}
          highlighted={isHighlighted(rn.node)}
          dimmed={isDimmed(rn.node)}
          ghost={rn.node.kind !== 'topic' && rn.node.support === 0}
          onClick={onNodeClick}
          onHover={() => {}}
        />
      {/each}
    </g>
  </svg>
</div>
