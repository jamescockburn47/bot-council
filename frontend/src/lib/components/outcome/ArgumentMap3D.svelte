<script lang="ts">
  import type { GraphEdge, GraphNode, GraphState, NodeKind } from '$lib/argument-graph/types';
  import { colourFor, truncate } from '$lib/argument-graph/types';

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

  // Per-kind node-volume values for 3d-force-graph. The library renders the
  // default node sphere with radius = cbrt(nodeVal), so these translate to
  // approximate world-unit radii: topic≈3.0, consensus≈1.6, contested≈1.8,
  // minority≈1.7. Contested slightly bigger so divergence reads first;
  // consensus deliberately the smallest so it doesn't dominate the scene.
  const NODE_VAL: Record<NodeKind, number> = {
    topic: 28,
    consensus: 4,
    contested: 6,
    minority: 5,
  };

  // Sprite text sizing in world units. The camera is ~200–400 units
  // away at default framing, so these map to ~10–18px on screen.
  const SHORT_LABEL_CHARS = 28;
  const SHORT_TEXT_HEIGHT = 2.4;
  const FULL_TEXT_HEIGHT = 1.8;
  const TOPIC_TEXT_HEIGHT = 3.2;

  // Distance the camera lands from a clicked node. Has to be large enough
  // for the full-text sprite (wrapped 44 chars, textHeight 1.8, ≈45 world
  // units wide) to fit the canvas at the 50° default FOV.
  const CLICK_ZOOM_DIST = 90;

  // LOD distance thresholds with hysteresis — prevent label-flicker when
  // the camera sits right on the boundary.
  const LOD_NEAR = 22;
  const LOD_FAR = 28;

  let container = $state<HTMLDivElement | undefined>();
  let fg: any = null;
  let rafHandle = 0;
  let resizeObserver: ResizeObserver | null = null;

  type ThreeModule = typeof import('three');
  let THREE: ThreeModule | null = null;
  // Each node group we add carries its two label sprites in userData so the
  // RAF loop can toggle them without traversing the whole scene.
  let nodeGroups: Array<{
    group: import('three').Group;
    shortSprite: import('three').Sprite;
    fullSprite: import('three').Sprite;
    showingFull: boolean;
  }> = [];

  /**
   * Produce a short label from a longer argument string.
   *
   * `deriveGraph` hands us a `.label` that's already a character-sliced
   * truncation of the full point, which often cuts mid-word. Here we at
   * least snap to a word boundary and, where possible, take only the
   * first clause (before a comma / period / semicolon). That turns
   * "AI will displace 30-50% of junior associates, within 5 years" into
   * "AI will displace 30-50%…" instead of "AI will displace 30-50% of j".
   *
   * This is a stopgap — the proper fix is an LLM-generated `headline`
   * field on each synthesis node, noted as a backend follow-up.
   */
  function pithy(s: string, maxChars = SHORT_LABEL_CHARS): string {
    if (!s) return '';
    const clause = s.split(/[.,;:!?]/)[0].trim();
    if (clause.length <= maxChars) return clause;
    const words = clause.split(/\s+/);
    let out = '';
    for (const w of words) {
      const next = (out + ' ' + w).trim();
      if (next.length > maxChars - 1) break;
      out = next;
    }
    if (!out) out = clause.slice(0, maxChars - 1);
    return out + '…';
  }

  function wrap(s: string, lineLen: number): string {
    const words = s.split(/\s+/);
    const lines: string[] = [];
    let cur = '';
    for (const w of words) {
      if ((cur + ' ' + w).trim().length > lineLen) {
        if (cur) lines.push(cur);
        cur = w;
      } else {
        cur = (cur + ' ' + w).trim();
      }
    }
    if (cur) lines.push(cur);
    return lines.join('\n');
  }

  function escapeHtml(s: string): string {
    return s
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  /**
   * Transform our GraphState into 3d-force-graph's `{nodes, links}` shape.
   * We pin the topic at the origin (fx/fy/fz) so d3-force can't drift it
   * away — that gives the camera a stable target to frame.
   */
  function shape(g: GraphState) {
    const visible = new Set(
      g.nodes
        .filter((n) => n.kind === 'topic' || !hiddenKinds.has(n.kind))
        .map((n) => n.id),
    );
    return {
      nodes: g.nodes
        .filter((n) => visible.has(n.id))
        .map((n) => {
          const base: any = { ...n };
          if (n.kind === 'topic') {
            base.fx = 0;
            base.fy = 0;
            base.fz = 0;
          }
          return base;
        }),
      links: g.edges
        .map((e) => {
          const src = typeof e.source === 'string' ? e.source : e.source.id;
          const tgt = typeof e.target === 'string' ? e.target : e.target.id;
          if (!visible.has(src) || !visible.has(tgt)) return null;
          return { ...e, source: src, target: tgt };
        })
        .filter((e): e is Exclude<typeof e, null> => e !== null),
    };
  }

  function nodeColour(n: GraphNode): string {
    if (highlightedSupporter) {
      if (n.kind === 'topic') return '#f5f5f5';
      return n.supporters.includes(highlightedSupporter)
        ? '#ffffff'
        : `${colourFor(n.kind)}44`;
    }
    if (n.kind === 'topic') return '#f5f5f5';
    return colourFor(n.kind);
  }

  function linkColour(e: GraphEdge): string {
    switch (e.kind) {
      case 'topic-consensus':
      case 'consensus-link':
        return 'rgba(16,185,129,0.45)';
      case 'topic-contested':
      case 'tension':
        return 'rgba(244,63,94,0.7)';
      case 'topic-minority':
        return 'rgba(139,92,246,0.55)';
    }
  }

  async function init() {
    if (!container) return;
    const [
      { default: ForceGraph3D },
      three,
      { default: SpriteText },
      d3f,
    ] = await Promise.all([
      import('3d-force-graph'),
      import('three'),
      import('three-spritetext'),
      import('d3-force-3d'),
    ]);
    THREE = three;

    fg = new ForceGraph3D(container)
      // 3d-force-graph's default canvas sizes to window.innerWidth ×
      // window.innerHeight, which overflows our bounded container (70vh,
      // page gutter). Pin the renderer to the container's current box and
      // let the ResizeObserver below keep it in sync.
      .width(container.clientWidth)
      .height(container.clientHeight)
      .backgroundColor('#0a0a11')
      .showNavInfo(false)
      .graphData(shape(graph) as any)
      .nodeId('id')
      .nodeVal((n: any) => NODE_VAL[(n as GraphNode).kind])
      .nodeColor((n: any) => nodeColour(n as GraphNode))
      // `extend(false)` REPLACES the default sphere with our group — no
      // more blob-under-the-text occlusion. The node's "presence" in the
      // scene is now entirely its label; edges still connect to the
      // logical node position (x/y/z), so the graph structure reads
      // through line geometry rather than through spheres.
      .nodeThreeObjectExtend(false)
      .nodeThreeObject((n: any) => {
        const node = n as GraphNode;
        const group = new THREE!.Group();

        if (node.kind === 'topic') {
          // Topic = a single permanent billboard centred at origin. No
          // sphere, no halo — the label IS the topic. Made larger and
          // bolder than argument labels so it reads as the focal point.
          const label = new SpriteText(wrap(node.fullText || 'Topic', 34));
          label.color = '#ffffff';
          label.backgroundColor = 'rgba(10,10,17,0.94)';
          label.borderColor = 'rgba(255,255,255,0.28)';
          label.borderWidth = 0.8;
          label.padding = 4;
          label.fontFace = 'ui-sans-serif, system-ui, -apple-system, sans-serif';
          label.fontWeight = '700';
          label.textHeight = TOPIC_TEXT_HEIGHT;
          group.add(label);
          return group;
        }

        // Argument nodes: short label always visible; full argument
        // appears when the camera gets close. Coloured border encodes
        // kind (consensus / contested / minority) so no sphere needed.
        const colour = colourFor(node.kind);
        const shortSprite = new SpriteText(
          pithy(node.fullText || node.label, SHORT_LABEL_CHARS),
        );
        shortSprite.color = '#e7e7ea';
        shortSprite.backgroundColor = 'rgba(10,10,17,0.78)';
        shortSprite.borderColor = `${colour}88`;
        shortSprite.borderWidth = 0.55;
        shortSprite.padding = 2.5;
        shortSprite.fontFace = 'ui-sans-serif, system-ui, -apple-system, sans-serif';
        shortSprite.textHeight = SHORT_TEXT_HEIGHT;

        const fullSprite = new SpriteText(wrap(node.fullText || node.label, 44));
        fullSprite.color = '#ffffff';
        fullSprite.backgroundColor = 'rgba(10,10,17,0.94)';
        fullSprite.borderColor = colour;
        fullSprite.borderWidth = 0.75;
        fullSprite.padding = 3;
        fullSprite.fontFace = 'ui-sans-serif, system-ui, -apple-system, sans-serif';
        fullSprite.textHeight = FULL_TEXT_HEIGHT;
        fullSprite.visible = false;

        group.add(shortSprite);
        group.add(fullSprite);
        nodeGroups.push({ group, shortSprite, fullSprite, showingFull: false });

        return group;
      })
      .linkColor((e: any) => linkColour(e as GraphEdge))
      .linkWidth((e: any) => ((e as GraphEdge).kind === 'tension' ? 0.6 : 0.4))
      .linkOpacity(0.75)
      .linkDirectionalParticles((e: any) =>
        (e as GraphEdge).kind === 'tension' ? 2 : 0,
      )
      .linkDirectionalParticleColor(() => 'rgba(244,63,94,0.95)')
      .linkDirectionalParticleWidth(0.9)
      .linkDirectionalParticleSpeed(0.006)
      .nodeLabel((n: any) => {
        const node = n as GraphNode;
        if (node.kind === 'topic') return '';
        const support = `${node.support}/${node.totalBots}`;
        const conf = node.confidence != null ? ` · conf ${node.confidence}` : '';
        const body = escapeHtml(node.fullText || node.label);
        return `<div style="background:#0a0a11;border:1px solid ${colourFor(node.kind)};padding:6px 9px;border-radius:6px;max-width:300px;font-family:ui-sans-serif;color:#e4e4e7;font-size:11px;line-height:1.4;"><div style="color:${colourFor(node.kind)};text-transform:uppercase;letter-spacing:0.08em;font-size:9px;margin-bottom:4px;">${node.kind} · ${support}${conf}</div>${body}</div>`;
      })
      .onNodeClick((n: any) => {
        // Stop CLICK_ZOOM_DIST world units *past* the node along the
        // radial from origin, so the full-text sprite (≈45 units wide)
        // fits the canvas with margin at the default 50° FOV. `dist=18`
        // used to put the camera right on top of the sprite and cropped
        // every long argument.
        const x = n.x ?? 0, y = n.y ?? 0, z = n.z ?? 0;
        const r = Math.max(Math.hypot(x, y, z), 0.1);
        const k = 1 + CLICK_ZOOM_DIST / r;
        fg.cameraPosition({ x: x * k, y: y * k, z: z * k }, n, 700);
        onNodeClick(n.id);
      })
      .onLinkClick((e: any) => {
        const edge = e as GraphEdge;
        if (edge.kind === 'tension') onEdgeClick(edge.id);
      });

    // Semantic force layout: encode each node-kind's meaning into
    // position so the user reads the figure even before the legend.
    //
    //   +y (up)    consensus — bots converged
    //   -y (down)  minority  — dissent preserved, isolated
    //   -x (left)  contested side_a
    //   +x (right) contested side_b
    //   origin    topic (pinned via fx/fy/fz in shape())
    //
    // Magnitudes are chosen so the shape reads at the default camera
    // distance without nodes clipping into the legend overlay.
    fg.d3Force(
      'consensus-y',
      d3f
        .forceY(100)
        .strength((n: any) => ((n as GraphNode).kind === 'consensus' ? 0.13 : 0)),
    )
      .d3Force(
        'minority-y',
        d3f
          .forceY(-100)
          .strength((n: any) => ((n as GraphNode).kind === 'minority' ? 0.13 : 0)),
      )
      .d3Force(
        'contested-a-x',
        d3f.forceX(-120).strength((n: any) => {
          const node = n as GraphNode;
          return node.kind === 'contested' && node.sideKey === 'a' ? 0.16 : 0;
        }),
      )
      .d3Force(
        'contested-b-x',
        d3f.forceX(120).strength((n: any) => {
          const node = n as GraphNode;
          return node.kind === 'contested' && node.sideKey === 'b' ? 0.16 : 0;
        }),
      );

    // Smoother orbit feel.
    try {
      const controls = fg.controls();
      if (controls) {
        controls.enableDamping = true;
        controls.dampingFactor = 0.12;
      }
    } catch {
      // fall back silently — 3d-force-graph sometimes swaps control types
    }

    // Keep the renderer sized to the container on window/layout resize
    // (sidebar collapse, devtools open, DPR change, etc.).
    const ro = new ResizeObserver(() => {
      if (!fg || !container) return;
      fg.width(container.clientWidth);
      fg.height(container.clientHeight);
    });
    ro.observe(container);
    resizeObserver = ro;

    // Camera sits on the +z axis looking at the origin, where the topic
    // is pinned. This keeps the topic exactly at the canvas centre
    // regardless of where d3-force pushes the argument nodes.
    //
    // We deliberately DON'T call `zoomToFit` — that would frame the
    // bounding-box centroid, which drifts when one kind of argument
    // dominates, pulling the topic off-centre. A fixed distance that
    // scales with node count covers every realistic graph.
    const nodeCount = graph.nodes.length;
    const camZ = Math.max(280, 160 + nodeCount * 18);
    fg.cameraPosition({ x: 0, y: 0, z: camZ }, { x: 0, y: 0, z: 0 });

    // LOD loop: reveal full argument text when the camera is close to a
    // node, hide it when far. Hysteresis band prevents flicker.
    const tick = () => {
      if (!fg) return;
      const camPos = fg.camera().position;
      for (const entry of nodeGroups) {
        const dx = entry.group.position.x - camPos.x;
        const dy = entry.group.position.y - camPos.y;
        const dz = entry.group.position.z - camPos.z;
        const d = Math.sqrt(dx * dx + dy * dy + dz * dz);
        if (!entry.showingFull && d < LOD_NEAR) {
          entry.showingFull = true;
          entry.shortSprite.visible = false;
          entry.fullSprite.visible = true;
        } else if (entry.showingFull && d > LOD_FAR) {
          entry.showingFull = false;
          entry.shortSprite.visible = true;
          entry.fullSprite.visible = false;
        }
      }
      rafHandle = requestAnimationFrame(tick);
    };
    rafHandle = requestAnimationFrame(tick);
  }

  function resetView() {
    if (!fg) return;
    const nodeCount = graph.nodes.length;
    const camZ = Math.max(280, 160 + nodeCount * 18);
    fg.cameraPosition({ x: 0, y: 0, z: camZ }, { x: 0, y: 0, z: 0 }, 600);
  }

  $effect(() => {
    void init();
    return () => {
      if (rafHandle) cancelAnimationFrame(rafHandle);
      if (resizeObserver) {
        resizeObserver.disconnect();
        resizeObserver = null;
      }
      nodeGroups = [];
      if (fg) {
        try {
          fg._destructor?.();
        } catch {
          // ignore
        }
        fg = null;
      }
    };
  });

  $effect(() => {
    if (!fg) return;
    nodeGroups = []; // rebuilt on data change via nodeThreeObject callback
    fg.graphData(shape(graph) as any);
    // No zoomToFit — camera target stays on origin (= pinned topic).
  });

  $effect(() => {
    if (!fg) return;
    highlightedSupporter; // touch for reactivity
    fg.nodeColor((n: any) => nodeColour(n as GraphNode));
  });
</script>

<div class="relative w-full mx-auto">
  <div
    bind:this={container}
    class="w-full rounded-xl border border-[var(--border)] overflow-hidden bg-[#0a0a11]"
    style="height: 70vh; min-height: 560px;"
  ></div>
  <button
    type="button"
    onclick={resetView}
    class="absolute bottom-3 right-3 text-[10px] mono uppercase tracking-wider px-2.5 py-1.5 rounded bg-black/60 backdrop-blur border border-white/10 text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-white/30 transition-colors"
    aria-label="Reset camera view"
  >
    Reset view
  </button>
</div>
