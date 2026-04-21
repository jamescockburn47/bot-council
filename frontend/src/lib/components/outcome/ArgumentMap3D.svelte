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

  // Sprite text sizing in world units. The camera is ~200 units away at
  // default framing, so these map to ~10–18px on screen.
  const SHORT_LABEL_CHARS = 28;
  const SHORT_TEXT_HEIGHT = 2.4;
  const FULL_TEXT_HEIGHT = 1.8;
  const TOPIC_TEXT_HEIGHT = 3.2;

  // LOD distance thresholds with hysteresis — prevent label-flicker when
  // the camera sits right on the boundary.
  const LOD_NEAR = 22;
  const LOD_FAR = 28;

  let container = $state<HTMLDivElement | undefined>();
  let fg: any = null;
  let rafHandle = 0;

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
    const [{ default: ForceGraph3D }, three, { default: SpriteText }] = await Promise.all([
      import('3d-force-graph'),
      import('three'),
      import('three-spritetext'),
    ]);
    THREE = three;

    fg = new ForceGraph3D(container)
      .backgroundColor('#0a0a11')
      .showNavInfo(false)
      .graphData(shape(graph) as any)
      .nodeId('id')
      .nodeVal((n: any) => NODE_VAL[(n as GraphNode).kind])
      .nodeColor((n: any) => nodeColour(n as GraphNode))
      .nodeOpacity(0.92)
      .nodeResolution(16)
      .nodeThreeObjectExtend(true)
      .nodeThreeObject((n: any) => {
        const node = n as GraphNode;
        const group = new THREE!.Group();

        if (node.kind === 'topic') {
          // Topic: a permanent, always-readable billboard with the full
          // question, plus a soft halo so the anchor is obvious from any
          // angle. The sphere itself comes from 3d-force-graph's default
          // (we use nodeThreeObjectExtend); we only add the label + halo.
          const halo = new THREE!.Mesh(
            new THREE!.SphereGeometry(4.5, 24, 24),
            new THREE!.MeshBasicMaterial({
              color: 0xffffff,
              transparent: true,
              opacity: 0.12,
            }),
          );
          group.add(halo);

          const label = new SpriteText(wrap(node.fullText || 'Topic', 34));
          label.color = '#ffffff';
          label.backgroundColor = 'rgba(10,10,17,0.92)';
          label.borderColor = 'rgba(255,255,255,0.18)';
          label.borderWidth = 0.5;
          label.padding = 3;
          label.fontFace = 'ui-sans-serif, system-ui, -apple-system, sans-serif';
          label.fontWeight = '600';
          label.textHeight = TOPIC_TEXT_HEIGHT;
          label.position.set(0, 7, 0);
          group.add(label);
          return group;
        }

        // Non-topic: short label (always readable from afar) and a full
        // label (kept hidden until the camera gets close). Swapped by the
        // RAF loop below rather than rebuilt, so zoom feels instant.
        const colour = colourFor(node.kind);
        const shortSprite = new SpriteText(truncate(node.label, SHORT_LABEL_CHARS));
        shortSprite.color = '#e7e7ea';
        shortSprite.backgroundColor = 'rgba(10,10,17,0.70)';
        shortSprite.borderColor = `${colour}55`;
        shortSprite.borderWidth = 0.35;
        shortSprite.padding = 2;
        shortSprite.fontFace = 'ui-sans-serif, system-ui, -apple-system, sans-serif';
        shortSprite.textHeight = SHORT_TEXT_HEIGHT;
        shortSprite.position.set(0, 3.5, 0);

        const fullSprite = new SpriteText(wrap(node.fullText || node.label, 44));
        fullSprite.color = '#ffffff';
        fullSprite.backgroundColor = 'rgba(10,10,17,0.94)';
        fullSprite.borderColor = colour;
        fullSprite.borderWidth = 0.5;
        fullSprite.padding = 3;
        fullSprite.fontFace = 'ui-sans-serif, system-ui, -apple-system, sans-serif';
        fullSprite.textHeight = FULL_TEXT_HEIGHT;
        fullSprite.position.set(0, 3.5, 0);
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
        // Dolly the camera toward the node but don't get so close that the
        // node itself clips out of view.
        const dist = 18;
        const x = n.x ?? 0, y = n.y ?? 0, z = n.z ?? 0;
        const r = Math.max(Math.hypot(x, y, z), 0.1);
        const k = 1 + dist / r;
        fg.cameraPosition({ x: x * k, y: y * k, z: z * k }, n, 700);
        onNodeClick(n.id);
      })
      .onLinkClick((e: any) => {
        const edge = e as GraphEdge;
        if (edge.kind === 'tension') onEdgeClick(edge.id);
      });

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

    // Initial camera: gentle top-down angle looking at origin (= topic).
    fg.cameraPosition({ x: 0, y: 60, z: 220 }, { x: 0, y: 0, z: 0 });

    // Once the physics has roughly settled, frame the whole graph. A short
    // timeout is simpler and more reliable than `onEngineStop`, which fires
    // before labels are laid out.
    setTimeout(() => {
      try {
        fg.zoomToFit(600, 60);
      } catch {
        // ignore — non-critical
      }
    }, 900);

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
    fg.cameraPosition({ x: 0, y: 60, z: 220 }, { x: 0, y: 0, z: 0 }, 600);
    setTimeout(() => {
      try {
        fg.zoomToFit(500, 60);
      } catch {
        // ignore
      }
    }, 650);
  }

  $effect(() => {
    void init();
    return () => {
      if (rafHandle) cancelAnimationFrame(rafHandle);
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
    setTimeout(() => {
      try {
        fg.zoomToFit(600, 60);
      } catch {
        // ignore
      }
    }, 700);
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
