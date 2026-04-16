<script lang="ts">
  import AgentBadge from '$lib/components/AgentBadge.svelte';
  import RawJsonToggle from '$lib/components/RawJsonToggle.svelte';
  import type { DivergenceEntry } from '$lib/types';

  let { analyses, roleMap }: {
    analyses: DivergenceEntry[];
    roleMap: Record<string, string | null>;
  } = $props();

  let expanded = $state(false);

  function magnitudeColor(mag: string | null): string {
    if (!mag) return '#94a3b8';
    switch (mag.toLowerCase()) {
      case 'major': return '#ef4444';
      case 'moderate': return '#f59e0b';
      case 'minor': return '#22c55e';
      default: return '#94a3b8';
    }
  }
</script>

<div class="border border-[var(--border)] rounded-lg overflow-hidden">
  <button
    onclick={() => (expanded = !expanded)}
    class="w-full flex items-center justify-between px-4 py-3 bg-[var(--surface)] hover:bg-[var(--surface-hover)] transition-colors text-left"
  >
    <div class="flex items-center gap-3">
      <span class="text-sm font-medium text-[var(--text-primary)]">Divergence Analysis</span>
      <span class="text-[10px] mono text-[var(--text-muted)]">
        {analyses.length} entr{analyses.length !== 1 ? 'ies' : 'y'}
      </span>
    </div>
    <span class="text-xs mono text-[var(--text-muted)]">{expanded ? '-' : '+'}</span>
  </button>

  {#if expanded}
    <div class="p-4 space-y-3">
      {#each analyses as entry (entry.pseudonym)}
        <div class="bg-[var(--bg)] border border-[var(--border)] rounded-lg p-3">
          <div class="flex items-center gap-3 mb-2">
            <AgentBadge pseudonym={entry.pseudonym} role={roleMap[entry.pseudonym] ?? null} />

            {#if entry.magnitude}
              {@const magColor = magnitudeColor(entry.magnitude)}
              <span
                class="text-[10px] mono px-1.5 py-0.5 rounded"
                style="color: {magColor}; background: {magColor}15; border: 1px solid {magColor}30;"
              >
                {entry.magnitude}
              </span>
            {/if}

            {#if entry.justification_adequate !== null}
              <span class="text-[10px] mono {entry.justification_adequate ? 'text-green-400' : 'text-red-400'}">
                {entry.justification_adequate ? 'justified' : 'unjustified'}
              </span>
            {/if}
          </div>

          {#if entry.what_changed}
            <p class="text-xs text-[var(--text-secondary)]">{entry.what_changed}</p>
          {/if}

          {#if entry.flags.length > 0}
            <div class="flex gap-1.5 mt-2 flex-wrap">
              {#each entry.flags as flag}
                <span class="text-[10px] mono px-1.5 py-0.5 rounded bg-red-500/10 text-red-400 border border-red-500/20">
                  {flag}
                </span>
              {/each}
            </div>
          {/if}
        </div>
      {/each}

      <RawJsonToggle data={analyses} />
    </div>
  {/if}
</div>
