<script lang="ts">
  let { title, count, color, items }: {
    title: string;
    count: number;
    color: string;
    items: { label: string; detail: string }[];
  } = $props();

  let expanded = $state(false);
</script>

<button
  onclick={() => (expanded = !expanded)}
  class="card-term card-term-hover w-full text-left"
  style="display: block;"
>
  <div class="flex items-center justify-between mb-2">
    <p class="tm-eyebrow" style="color: {color}; margin-bottom: 0;">{title}</p>
    <span class="stat-serif" style="font-size: 28px; color: {color};">{count}</span>
  </div>

  {#if !expanded && items.length > 0}
    <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-dim); display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;">
      {items[0].label}: {items[0].detail}
    </p>
  {/if}

  {#if expanded}
    <div class="mt-3 space-y-2">
      {#each items as item}
        <div class="pl-3 py-1" style="border-left: 2px solid color-mix(in srgb, {color} 30%, transparent);">
          <p style="font-family: var(--sans-product); font-size: 13px; font-weight: 500; color: var(--glow-txt);">{item.label}</p>
          <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-dim); margin-top: 2px;">{item.detail}</p>
        </div>
      {/each}
    </div>
  {/if}

  <div class="mt-2 flex justify-end">
    <span class="mono-label">
      {expanded ? 'Collapse' : `${items.length} item${items.length !== 1 ? 's' : ''}`}
    </span>
  </div>
</button>
