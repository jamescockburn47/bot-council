<script lang="ts">
  import type { NodeKind } from '$lib/argument-graph/types';

  let {
    hiddenKinds,
    supporters,
    highlightedSupporter,
    onToggleKind,
    onSupporterChange,
  }: {
    hiddenKinds: Set<NodeKind>;
    supporters: string[];
    highlightedSupporter: string | null;
    onToggleKind: (kind: NodeKind) => void;
    onSupporterChange: (pseudo: string | null) => void;
  } = $props();

  function buttonClass(active: boolean): string {
    return [
      'text-[10px] mono uppercase tracking-wider px-2.5 py-1 rounded transition-colors',
      active
        ? 'bg-[var(--text-primary)]/10 text-[var(--text-primary)] border border-[var(--text-muted)]'
        : 'text-[var(--text-muted)] border border-[var(--border)] hover:text-[var(--text-primary)] hover:border-[var(--text-muted)]',
    ].join(' ');
  }
</script>

<div class="flex items-center gap-2 flex-wrap mb-3">
  <button
    type="button"
    class={buttonClass(hiddenKinds.has('minority'))}
    onclick={() => onToggleKind('minority')}
    aria-pressed={hiddenKinds.has('minority')}
  >
    {hiddenKinds.has('minority') ? 'Show' : 'Hide'} minority
  </button>
  <button
    type="button"
    class={buttonClass(hiddenKinds.has('contested'))}
    onclick={() => onToggleKind('contested')}
    aria-pressed={hiddenKinds.has('contested')}
  >
    {hiddenKinds.has('contested') ? 'Show' : 'Hide'} contested
  </button>

  <div class="flex-1"></div>

  <label class="flex items-center gap-2 text-[10px] mono text-[var(--text-muted)]">
    <span>Highlight supporter</span>
    <select
      class="bg-[var(--surface)] border border-[var(--border)] rounded px-2 py-1 text-[var(--text-secondary)] text-[10px] mono"
      value={highlightedSupporter ?? ''}
      onchange={(e) => {
        const v = (e.currentTarget as HTMLSelectElement).value;
        onSupporterChange(v === '' ? null : v);
      }}
    >
      <option value="">(none)</option>
      {#each supporters as p (p)}
        <option value={p}>{p}</option>
      {/each}
    </select>
  </label>
</div>
