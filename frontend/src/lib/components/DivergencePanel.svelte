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
    if (!mag) return '#8888A0';
    switch (mag.toLowerCase()) {
      case 'major': return '#ef4444';
      case 'moderate': return '#f59e0b';
      case 'minor': return '#22c55e';
      default: return '#8888A0';
    }
  }

  function magnitudeTrend(mag: string | null): string {
    if (!mag) return '';
    switch (mag.toLowerCase()) {
      case 'major': return '↑';
      case 'moderate': return '→';
      case 'minor': return '↓';
      default: return '';
    }
  }
</script>

<div class="card-term-lg" style="padding: 0; overflow: hidden;">
  <button
    onclick={() => (expanded = !expanded)}
    class="w-full flex items-center justify-between px-6 py-4 text-left transition-colors"
    style="background: var(--night-raise);"
    onmouseenter={(e) => (e.currentTarget.style.background = 'var(--night-edge)')}
    onmouseleave={(e) => (e.currentTarget.style.background = 'var(--night-raise)')}
  >
    <div class="flex items-center gap-4">
      <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 0;">Divergence Analysis</p>
      <span class="stat-serif" style="font-size: 40px; line-height: 1;">{analyses.length}</span>
      <span class="mono-label">{analyses.length !== 1 ? 'entries' : 'entry'}</span>
    </div>
    <span class="mono-label">{expanded ? '-' : '+'}</span>
  </button>

  {#if expanded}
    <div class="p-6 space-y-3" style="border-top: 1px solid var(--night-rule);">
      {#each analyses as entry (entry.pseudonym)}
        <div class="card-term">
          <div class="flex items-center gap-3 mb-2">
            <AgentBadge pseudonym={entry.pseudonym} role={roleMap[entry.pseudonym] ?? null} />

            {#if entry.magnitude}
              {@const magColor = magnitudeColor(entry.magnitude)}
              {@const magTrend = magnitudeTrend(entry.magnitude)}
              <span
                class="mono-label"
                style="padding: 2px 6px; border-radius: var(--r-sm); color: {magColor}; background: color-mix(in srgb, {magColor} 15%, transparent); border: 1px solid color-mix(in srgb, {magColor} 30%, transparent);"
              >
                {#if magTrend}<span style="font-family: var(--mono-product);">{magTrend}</span>{/if}
                {entry.magnitude}
              </span>
            {/if}

            {#if entry.justification_adequate !== null}
              <span
                class="mono-label"
                style="color: {entry.justification_adequate ? '#4ade80' : '#f87171'};"
              >
                {entry.justification_adequate ? 'justified' : 'unjustified'}
              </span>
            {/if}
          </div>

          {#if entry.what_changed}
            <p style="font-family: var(--sans-product); font-size: 14px; line-height: 1.6; color: var(--glow-dim);">{entry.what_changed}</p>
          {/if}

          {#if entry.flags.length > 0}
            <div class="flex gap-1.5 mt-2 flex-wrap">
              {#each entry.flags as flag}
                <span
                  class="mono-label"
                  style="padding: 2px 6px; border-radius: var(--r-sm); color: #f87171; background: rgba(239,68,68,0.10); border: 1px solid rgba(239,68,68,0.20);"
                >
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
