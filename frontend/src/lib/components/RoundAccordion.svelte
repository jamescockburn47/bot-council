<script lang="ts">
  import ResponseCard from '$lib/components/ResponseCard.svelte';
  import RawJsonToggle from '$lib/components/RawJsonToggle.svelte';
  import type { TranscriptRound, CruxData } from '$lib/types';

  let { round, roleMap, crux = null }: {
    round: TranscriptRound;
    roleMap: Record<string, string | null>;
    crux?: CruxData | null;
  } = $props();

  let expanded = $state(false);

  const ROUND_NAMES: Record<number, string> = {
    0: 'Blind Formation',
    1: 'Anonymous Distribution',
    2: 'Structured Rebuttal',
    3: 'Cross-Examination',
    4: 'Final Position',
  };

  let roundName = $derived(ROUND_NAMES[round.round_number] ?? `Round ${round.round_number}`);
  let isComplete = $derived(round.status === 'complete');
  let isPending = $derived(round.status === 'pending');
</script>

<div class="border border-[var(--border)] rounded-lg overflow-hidden">
  <button
    onclick={() => (expanded = !expanded)}
    class="w-full flex items-center justify-between px-4 py-3 bg-[var(--surface)] hover:bg-[var(--surface-hover)] transition-colors text-left"
  >
    <div class="flex items-center gap-3">
      <span class="text-xs mono text-[var(--text-muted)]">R{round.round_number}</span>
      <span class="text-sm font-medium text-[var(--text-primary)]">{roundName}</span>
      <span class="text-[10px] mono text-[var(--text-muted)]">
        {round.responses.length} response{round.responses.length !== 1 ? 's' : ''}
      </span>
    </div>
    <div class="flex items-center gap-2">
      {#if isComplete}
        <span class="w-2 h-2 rounded-full bg-green-500"></span>
      {:else if isPending}
        <span class="w-2 h-2 rounded-full bg-[var(--text-muted)]"></span>
      {:else}
        <span class="w-2 h-2 rounded-full bg-[#8b5cf6] animate-pulse"></span>
      {/if}
      <span class="text-xs mono text-[var(--text-muted)]">{expanded ? '-' : '+'}</span>
    </div>
  </button>

  {#if expanded}
    <div class="p-4 space-y-3">
      {#if isPending}
        <p class="text-xs text-[var(--text-muted)] mono italic">Pending</p>
      {:else if !isComplete}
        <p class="text-xs text-[#8b5cf6] mono italic">In progress...</p>
      {/if}

      {#if crux && round.round_number === 3}
        <div
          class="bg-[#8b5cf615] border border-[#8b5cf630] rounded-lg p-4 mb-1"
        >
          <h3
            class="text-xs mono uppercase tracking-wider text-[var(--text-muted)] mb-1"
          >
            Crux
          </h3>
          <p class="text-sm text-[var(--text-primary)]">{crux.claim}</p>
          <p class="text-xs text-[var(--text-muted)] mt-2">
            First stated by {crux.source_pseudonym}
          </p>
          <p
            class="text-[11px] text-[var(--text-muted)] mt-1.5 italic"
          >
            <span class="mono uppercase tracking-wider text-[10px] not-italic">Source quote:</span>
            &ldquo;{crux.source_quote}&rdquo;
          </p>
        </div>
      {/if}

      {#each round.responses as entry (entry.pseudonym)}
        <ResponseCard {entry} {roleMap} roundNumber={round.round_number} />
      {/each}

      <RawJsonToggle data={round} />
    </div>
  {/if}
</div>
