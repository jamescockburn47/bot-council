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
  class="w-full text-left bg-[var(--surface)] border border-[var(--border)] rounded-lg p-4 hover:border-[{color}]/40 transition-colors"
>
  <div class="flex items-center justify-between mb-2">
    <h4 class="text-xs mono uppercase tracking-wider" style="color: {color};">{title}</h4>
    <span
      class="text-lg font-bold mono"
      style="color: {color};"
    >{count}</span>
  </div>

  {#if !expanded && items.length > 0}
    <p class="text-xs text-[var(--text-secondary)] line-clamp-2">
      {items[0].label}: {items[0].detail}
    </p>
  {/if}

  {#if expanded}
    <div class="mt-3 space-y-2">
      {#each items as item}
        <div class="border-l-2 pl-3 py-1" style="border-color: {color}30;">
          <p class="text-xs font-medium text-[var(--text-primary)]">{item.label}</p>
          <p class="text-xs text-[var(--text-secondary)] mt-0.5">{item.detail}</p>
        </div>
      {/each}
    </div>
  {/if}

  <div class="mt-2 flex justify-end">
    <span class="text-[10px] mono text-[var(--text-muted)]">
      {expanded ? 'Collapse' : `${items.length} item${items.length !== 1 ? 's' : ''}`}
    </span>
  </div>
</button>
