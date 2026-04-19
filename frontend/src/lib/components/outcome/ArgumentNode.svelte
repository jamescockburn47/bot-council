<script lang="ts">
  import type { GraphNode } from '$lib/argument-graph/types';
  import { colourFor } from '$lib/argument-graph/types';
  import { nodeRadius } from '$lib/argument-graph/simulation';

  let {
    node,
    selected,
    highlighted,
    dimmed,
    ghost,
    onClick,
    onHover,
  }: {
    node: GraphNode;
    selected: boolean;
    highlighted: boolean;
    dimmed: boolean;
    ghost: boolean;
    onClick: (id: string) => void;
    onHover: (id: string | null) => void;
  } = $props();

  let x = $derived(node.x ?? 0);
  let y = $derived(node.y ?? 0);
  let r = $derived(nodeRadius(node));
  let colour = $derived(colourFor(node.kind));
  let opacity = $derived(ghost ? 0.25 : dimmed ? 0.2 : 1);
</script>

<g
  role="button"
  tabindex="0"
  aria-label="{node.label} — support {node.support} of {node.totalBots}"
  style="cursor: pointer; opacity: {opacity}; transition: opacity 220ms ease;"
  onclick={() => onClick(node.id)}
  onkeydown={(e) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onClick(node.id);
    }
  }}
  onmouseenter={() => onHover(node.id)}
  onmouseleave={() => onHover(null)}
  onfocus={() => onHover(node.id)}
  onblur={() => onHover(null)}
>
  <!-- Outer halo -->
  {#if !ghost}
    <circle
      cx={x}
      cy={y}
      r={r + 14}
      fill={colour}
      opacity="0.12"
      filter="url(#am-halo)"
    />
  {/if}

  <!-- Main disc -->
  <circle
    cx={x}
    cy={y}
    r={r}
    fill={node.kind === 'topic' ? 'url(#am-topic-grad)' : colour}
    stroke={selected || highlighted ? '#fafafa' : colour}
    stroke-width={selected ? 2 : highlighted ? 1.5 : 1}
    stroke-opacity={selected ? 1 : 0.85}
  />

  <!-- Inner highlight (non-topic) -->
  {#if node.kind !== 'topic' && !ghost}
    <circle cx={x} cy={y - r * 0.3} r={r * 0.45} fill="#ffffff" opacity="0.12" />
  {/if}

  <!-- Labels -->
  {#if !ghost && node.kind !== 'topic'}
    <text
      x={x}
      y={y - r - 10}
      text-anchor="middle"
      fill="#e4e4e7"
      font-size="11"
      font-family="ui-sans-serif, system-ui, sans-serif"
      font-weight="500"
    >
      {node.label}
    </text>
    <text
      x={x}
      y={y - r - 22}
      text-anchor="middle"
      fill={colour}
      font-size="9"
      font-family="ui-monospace, SF Mono, monospace"
      opacity="0.7"
    >
      {node.support} of {node.totalBots}{node.confidence != null
        ? ` · conf ${node.confidence}`
        : ''}
    </text>
  {:else if node.kind === 'topic'}
    <text
      x={x}
      y={y + 1}
      text-anchor="middle"
      fill="#0a0a0d"
      font-size="10"
      font-family="ui-sans-serif, system-ui, sans-serif"
      font-weight="600"
      letter-spacing="0.05em"
    >
      TOPIC
    </text>
  {/if}
</g>
