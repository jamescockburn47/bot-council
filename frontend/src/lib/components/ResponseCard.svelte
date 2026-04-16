<script lang="ts">
  import AgentBadge from '$lib/components/AgentBadge.svelte';
  import ChallengeBlock from '$lib/components/ChallengeBlock.svelte';
  import PositionChangeBlock from '$lib/components/PositionChangeBlock.svelte';
  import type { TranscriptEntry } from '$lib/types';

  let { entry, roleMap, roundNumber }: {
    entry: TranscriptEntry;
    roleMap: Record<string, string | null>;
    roundNumber: number;
  } = $props();

  let role = $derived(roleMap[entry.pseudonym] ?? null);
</script>

<div class="bg-[var(--bg)] border border-[var(--border)] rounded-lg p-4">
  <div class="flex items-center justify-between mb-2">
    <div class="flex items-center gap-3">
      <AgentBadge pseudonym={entry.pseudonym} {role} />

      {#if entry.abstained && !entry.valid}
        <span class="text-[10px] mono px-1.5 py-0.5 rounded bg-red-500/10 text-red-400 border border-red-500/20">
          Unresponsive
        </span>
      {:else if entry.abstained}
        <span class="text-[10px] mono px-1.5 py-0.5 rounded bg-[var(--border)] text-[var(--text-muted)]">
          Abstained
        </span>
      {/if}
    </div>

    <div class="flex items-center gap-3">
      {#if entry.confidence !== null}
        <span class="text-xs mono text-[var(--text-muted)]">
          conf: <span class="text-[var(--text-secondary)]">{entry.confidence}</span>
        </span>
      {/if}
      {#if entry.valid}
        <span class="text-[10px] text-green-400">valid</span>
      {:else if !entry.abstained}
        <span class="text-[10px] text-red-400">invalid</span>
      {/if}
    </div>
  </div>

  {#if !entry.abstained}
    <p class="text-sm text-[var(--text-secondary)] whitespace-pre-wrap leading-relaxed">
      {entry.response}
    </p>
  {/if}

  {#if entry.challenge}
    <ChallengeBlock
      challenge={entry.challenge}
      validationReasoning={entry.validation_reasoning}
    />
  {/if}

  {#if entry.position_change}
    <PositionChangeBlock change={entry.position_change} />
  {/if}
</div>
