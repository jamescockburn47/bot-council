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

<div class="card-term" style="padding: 0; overflow: hidden;">
  <button
    onclick={() => (expanded = !expanded)}
    class="w-full flex items-center justify-between px-4 py-3 text-left transition-colors"
    style="background: var(--night-raise);"
    onmouseenter={(e) => (e.currentTarget.style.background = 'var(--night-edge)')}
    onmouseleave={(e) => (e.currentTarget.style.background = 'var(--night-raise)')}
  >
    <div class="flex items-center gap-3">
      <!-- R{n} pill -->
      <span
        style="
          font-family: var(--mono-product);
          font-size: 11px;
          font-weight: 500;
          color: var(--indigo-400);
          background: rgba(99,102,241,0.10);
          border: 1px solid rgba(99,102,241,0.25);
          border-radius: 999px;
          padding: 3px 8px;
          letter-spacing: 0.05em;
        "
      >R{round.round_number}</span>
      <span style="font-family: var(--sans-product); font-weight: 600; font-size: 14px; color: var(--glow-txt);">{roundName}</span>
      <span class="mono-label">{round.responses.length} response{round.responses.length !== 1 ? 's' : ''}</span>
    </div>
    <div class="flex items-center gap-2">
      {#if isComplete}
        <span class="w-2 h-2 rounded-full bg-green-500"></span>
      {:else if isPending}
        <span class="w-2 h-2 rounded-full" style="background: var(--glow-faint);"></span>
      {:else}
        <span class="w-2 h-2 rounded-full animate-pulse" style="background: var(--indigo-400);"></span>
      {/if}
      <span class="mono-label">{expanded ? '-' : '+'}</span>
    </div>
  </button>

  {#if expanded}
    <div class="p-5 space-y-3" style="border-top: 1px solid var(--night-rule);">
      {#if isPending}
        <p class="mono-label italic">Pending</p>
      {:else if !isComplete}
        <p class="mono-label italic" style="color: var(--indigo-400);">In progress...</p>
      {/if}

      {#if crux && round.round_number === 3}
        <div
          style="
            background: rgba(99,102,241,0.08);
            border: 1px solid rgba(99,102,241,0.25);
            border-radius: var(--r-md);
            padding: 16px;
            margin-bottom: 4px;
          "
        >
          <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 6px;">Crux</p>
          <p style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-txt);">{crux.claim}</p>
          <p class="mono-label" style="margin-top: 8px;">
            First stated by {crux.source_pseudonym}
          </p>
          <p
            style="font-family: var(--sans-product); font-style: italic; font-size: 12px; color: var(--glow-mute); margin-top: 6px;"
          >
            <span class="mono-label not-italic">Source quote:</span>
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
