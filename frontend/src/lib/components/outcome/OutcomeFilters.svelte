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
    return active ? 'pill-on' : 'pill-off';
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

  <label class="flex items-center gap-2" style="font-family: var(--mono-product); font-size: 10px; letter-spacing: 0.2em; text-transform: uppercase; color: var(--glow-mute);">
    <span>Highlight supporter</span>
    <select
      style="background: var(--night-raise); border: 1px solid var(--night-rule2); border-radius: 8px; padding: 8px 12px; font-family: var(--sans-product); font-size: 13px; color: var(--glow-txt);"
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
