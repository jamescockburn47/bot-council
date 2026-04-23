<script lang="ts">
  import AgentBadge from '$lib/components/AgentBadge.svelte';
  import ChallengeBlock from '$lib/components/ChallengeBlock.svelte';
  import PositionChangeBlock from '$lib/components/PositionChangeBlock.svelte';
  import SteelmanBlock from '$lib/components/SteelmanBlock.svelte';
  import type { TranscriptEntry } from '$lib/types';

  let { entry, roleMap, roundNumber }: {
    entry: TranscriptEntry;
    roleMap: Record<string, string | null>;
    roundNumber: number;
  } = $props();

  let role = $derived(roleMap[entry.pseudonym] ?? null);
  let steelmanMetadata = $derived(entry.extraction_metadata?.steelman ?? null);
  let showSteelman = $derived(
    roundNumber === 4 &&
      steelmanMetadata != null &&
      steelmanMetadata.source !== 'extraction_failed',
  );
  let carriedFromRound = $derived(
    entry.fallback_from_round != null ? entry.fallback_from_round : null,
  );
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

      {#if carriedFromRound != null}
        <span
          class="text-[10px] mono px-1.5 py-0.5 rounded text-[var(--text-muted)] border border-[var(--border)]"
          title="This bot did not respond in this round; its round-{carriedFromRound} position is shown so the voice is not lost."
        >
          &#x21bb; carried from R{carriedFromRound}
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
    {#if carriedFromRound != null}
      <!-- Carry-forward responses are NOT the bot's engagement with this
           round — they're the bot's round-{carriedFromRound} position
           preserved so the synthesis layer can still cite a voice.
           Rendering the R0 prose verbatim here was misleading (reads as
           substantive participation). Show a placeholder instead; the
           bot's R{carriedFromRound} answer is a click away on its own
           round card. -->
      <p class="text-xs text-[var(--text-muted)] italic leading-relaxed">
        {entry.pseudonym} did not substantively respond in this round.
        Their round-{carriedFromRound} position is preserved for synthesis.
      </p>
    {:else}
      <p class="text-sm text-[var(--text-secondary)] whitespace-pre-wrap leading-relaxed">
        {entry.response}
      </p>
    {/if}
  {/if}

  {#if entry.challenge}
    <ChallengeBlock
      challenge={entry.challenge}
      validationReasoning={entry.validation_reasoning}
      provenance={entry.extraction_metadata?.challenge ?? null}
    />
  {/if}

  {#if entry.position_change}
    <PositionChangeBlock
      change={entry.position_change}
      provenance={entry.extraction_metadata?.position_change ?? null}
    />
  {/if}

  {#if showSteelman && steelmanMetadata}
    <SteelmanBlock metadata={steelmanMetadata} />
  {/if}
</div>
