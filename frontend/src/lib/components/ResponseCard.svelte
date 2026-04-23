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

<div class="card-term">
  <div class="flex items-center justify-between mb-2">
    <div class="flex items-center gap-3">
      <AgentBadge pseudonym={entry.pseudonym} {role} />

      {#if entry.abstained && !entry.valid}
        <span
          class="mono-label"
          style="padding: 2px 6px; border-radius: var(--r-sm); color: #f87171; background: rgba(239,68,68,0.10); border: 1px solid rgba(239,68,68,0.20);"
        >
          Unresponsive
        </span>
      {:else if entry.abstained}
        <span
          class="mono-label"
          style="padding: 2px 6px; border-radius: var(--r-sm); background: var(--night-rule); color: var(--glow-faint);"
        >
          Abstained
        </span>
      {/if}

      {#if carriedFromRound != null}
        <span
          class="mono-label"
          style="padding: 2px 6px; border-radius: var(--r-sm); border: 1px solid var(--night-rule); color: var(--glow-faint);"
          title="This bot did not respond in this round; its round-{carriedFromRound} position is shown so the voice is not lost."
        >
          &#x21bb; carried from R{carriedFromRound}
        </span>
      {/if}
    </div>

    <div class="flex items-center gap-3">
      {#if entry.confidence !== null}
        <span class="mono-label">
          conf: <span style="color: var(--glow-dim);">{entry.confidence}</span>
        </span>
      {/if}
      {#if entry.valid}
        <span class="mono-label" style="color: #4ade80;">valid</span>
      {:else if !entry.abstained}
        <span class="mono-label" style="color: #f87171;">invalid</span>
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
      <p class="mono-label italic leading-relaxed">
        {entry.pseudonym} did not substantively respond in this round.
        Their round-{carriedFromRound} position is preserved for synthesis.
      </p>
    {:else}
      <p style="font-family: var(--sans-product); font-size: 15px; line-height: 1.65; color: var(--glow-dim); white-space: pre-wrap;">
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
