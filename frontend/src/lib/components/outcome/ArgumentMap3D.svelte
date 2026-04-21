<script lang="ts">
  import type { GraphEdge, GraphNode, GraphState, NodeKind } from '$lib/argument-graph/types';
  import { colourFor, truncate } from '$lib/argument-graph/types';
  import { nodeRadius } from '$lib/argument-graph/simulation';

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

  // 3d-force-graph pulls in three.js + the physics engine. Dynamic-import
  // it inside the client-side $effect so the static-adapter build doesn't
  // try to evaluate the library during prerender and so the ~800KB of
  // three.js is lazy-loaded for users who never open a debate page.
  let container = $state<HTMLDivElement | undefined>();
  let fg: any = null;

  type ThreeModule = typeof import('three');
  let THREE: ThreeModule | null = null;

  /// Shape 3d-force-graph expects. We don't mutate our GraphState — we
  /// copy into this format so the library's own layout doesn't bleed
  /// positions back into our domain objects. Filtering happens here too.
  function shape(g: GraphState) {
    const visible = new Set(
      g.nodes
        .filter((n) => n.kind === 'topic' || !hiddenKinds.has(n.kind))
        .map((n) => n.id),
    );
    return {
      nodes: g.nodes
        .filter((n) => visible.has(n.id))
        .map((n) => ({ ...n })),
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
      if (n.kind === 'topic') return '#fafafa';
      return n.supporters.includes(highlightedSupporter)
        ? '#fafafa'
        : `${colourFor(n.kind)}55`; // dim non-supporter nodes
    }
    if (n.kind === 'topic') return '#f5f5f5';
    return colourFor(n.kind);
  }

  function linkColour(e: GraphEdge): string {
    switch (e.kind) {
      case 'topic-consensus':
      case 'consensus-link':
        return 'rgba(16,185,129,0.55)';
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

    const g = shape(graph);
    fg = new ForceGraph3D(container)
      .backgroundColor('#0b0b11')
      .showNavInfo(false)
      // Our GraphNode/GraphEdge shapes carry richer fields than 3d-force-graph
      // models; the library still accepts extra properties, but its types are
      // strict. Cast at this interop boundary rather than polluting our domain
      // types with optional fields.
      .graphData(g as any)
      .nodeId('id')
      .nodeVal((n: any) => nodeRadius(n as GraphNode) * 6)
      .nodeColor((n: any) => nodeColour(n as GraphNode))
      .nodeLabel((n: any) => {
        const node = n as GraphNode;
        if (node.kind === 'topic') return '';
        const support = `${node.support}/${node.totalBots}`;
        const conf = node.confidence != null ? ` · conf ${node.confidence}` : '';
        return `<div style="background:#0a0a0e;border:1px solid ${colourFor(node.kind)};padding:6px 10px;border-radius:6px;max-width:320px;font-family:ui-sans-serif;color:#e4e4e7;font-size:11px;line-height:1.4;"><div style="color:${colourFor(node.kind)};text-transform:uppercase;letter-spacing:0.08em;font-size:9px;margin-bottom:4px;">${node.kind} · ${support}${conf}</div>${escapeHtml(node.fullText || node.label)}</div>`;
      })
      .nodeThreeObject((n: any) => {
        const node = n as GraphNode;
        // THREE is populated before this callback is first invoked (the
        // forward reference is set above, awaited alongside the library).
        const group = new THREE!.Group();
        if (node.kind === 'topic') {
          // Central topic: big glowing sphere + floating billboard with the
          // question text. The label is positioned above the sphere so it's
          // always obvious what the debate is about.
          const r = 26;
          const geom = new THREE!.SphereGeometry(r, 32, 32);
          const mat = new THREE!.MeshBasicMaterial({
            color: 0xfafafa,
            transparent: true,
            opacity: 0.92,
          });
          const sphere = new THREE!.Mesh(geom, mat);
          group.add(sphere);
          const halo = new THREE!.Mesh(
            new THREE!.SphereGeometry(r * 1.8, 32, 32),
            new THREE!.MeshBasicMaterial({
              color: 0xfafafa,
              transparent: true,
              opacity: 0.08,
            }),
          );
          group.add(halo);
          const label = new SpriteText(wrap(node.fullText || 'TOPIC', 36));
          label.color = '#ffffff';
          label.backgroundColor = 'rgba(10,10,14,0.85)';
          label.borderColor = 'rgba(255,255,255,0.12)';
          label.borderWidth = 0.6;
          label.padding = 4;
          label.fontFace = 'ui-sans-serif, system-ui, sans-serif';
          label.fontWeight = '600';
          label.textHeight = 5.5;
          label.position.set(0, r + 18, 0);
          group.add(label);
          return group;
        }
        // Non-topic nodes: default sphere rendered by 3d-force-graph is
        // fine (nodeVal + nodeColor handle it). Return an empty group so
        // the library falls back to its default; but we also add a small
        // overhead label so arguments are legible without hovering.
        const short = new SpriteText(truncate(node.label, 46));
        short.color = '#e4e4e7';
        short.backgroundColor = 'rgba(10,10,14,0.6)';
        short.borderColor = `${colourFor(node.kind)}66`;
        short.borderWidth = 0.4;
        short.padding = 2;
        short.fontFace = 'ui-sans-serif, system-ui, sans-serif';
        short.textHeight = 3.2;
        short.position.set(0, nodeRadius(node) + 6, 0);
        group.add(short);
        return group;
      })
      .nodeThreeObjectExtend(true) // keep default sphere + add our label/group
      .linkColor((e: any) => linkColour(e as GraphEdge))
      .linkWidth((e: any) => ((e as GraphEdge).kind === 'tension' ? 1.6 : 1.1))
      .linkOpacity(0.85)
      .linkDirectionalParticles((e: any) =>
        (e as GraphEdge).kind === 'tension' ? 3 : 0,
      )
      .linkDirectionalParticleColor(() => 'rgba(244,63,94,0.95)')
      .linkDirectionalParticleWidth(1.8)
      .linkDirectionalParticleSpeed(0.01)
      .onNodeClick((n: any) => {
        // Aim the camera at the clicked node and pull back to frame it.
        const distance = 180;
        const x = n.x ?? 0;
        const y = n.y ?? 0;
        const z = n.z ?? 0;
        const ratio = 1 + distance / Math.max(Math.hypot(x, y, z), 1);
        fg.cameraPosition({ x: x * ratio, y: y * ratio, z: z * ratio }, n, 800);
        onNodeClick(n.id);
      })
      .onLinkClick((e: any) => {
        const edge = e as GraphEdge;
        if (edge.kind === 'tension') onEdgeClick(edge.id);
      });

    // Start camera angled above the plane rather than flat-on so the 3D
    // shape reads as 3D from first paint.
    fg.cameraPosition({ x: 0, y: 160, z: 360 });
  }

  $effect(() => {
    void init();
    return () => {
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

  // Re-push data when the graph or filters change (no rebuild, same sim).
  $effect(() => {
    if (!fg) return;
    const g = shape(graph);
    fg.graphData(g as any);
  });

  // Re-colour when the highlighted supporter changes.
  $effect(() => {
    if (!fg) return;
    highlightedSupporter; // touch for reactivity
    fg.nodeColor((n: any) => nodeColour(n as GraphNode));
  });

  function escapeHtml(s: string): string {
    return s
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
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
</script>

<div
  bind:this={container}
  class="w-full rounded-xl border border-[var(--border)] overflow-hidden bg-[#0b0b11]"
  style="height: 600px;"
></div>
