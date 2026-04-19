<script lang="ts">
  type Tab = { id: string; label: string; disabled?: boolean };
  let {
    tabs,
    active,
    onChange,
  }: {
    tabs: Tab[];
    active: string;
    onChange: (id: string) => void;
  } = $props();
</script>

<div
  class="flex items-center gap-1 border-b border-[var(--border)] mb-6"
  role="tablist"
  aria-label="Debate view"
>
  {#each tabs as tab (tab.id)}
    <button
      type="button"
      role="tab"
      aria-selected={active === tab.id}
      aria-controls="debate-panel-{tab.id}"
      tabindex={active === tab.id ? 0 : -1}
      disabled={tab.disabled}
      onclick={() => !tab.disabled && onChange(tab.id)}
      class="relative px-4 py-2.5 text-xs mono uppercase tracking-wider transition-colors
             {active === tab.id
               ? 'text-[var(--text-primary)]'
               : 'text-[var(--text-muted)] hover:text-[var(--text-secondary)]'}
             {tab.disabled ? 'opacity-40 cursor-not-allowed' : ''}"
    >
      {tab.label}
      {#if active === tab.id}
        <span
          class="absolute left-0 right-0 -bottom-px h-px bg-[var(--text-primary)]"
          aria-hidden="true"
        ></span>
      {/if}
    </button>
  {/each}
</div>
