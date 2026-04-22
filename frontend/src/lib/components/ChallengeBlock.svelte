<script lang="ts">
  import { CHALLENGE_COLORS } from '$lib/utils/agent-colors';
  import type { ChallengeData, ExtractionProvenance } from '$lib/types';

  let { challenge, validationReasoning = null, provenance = null }: {
    challenge: ChallengeData;
    validationReasoning?: string | null;
    provenance?: ExtractionProvenance | null;
  } = $props();

  let showReasoning = $state(false);
  let borderColor = $derived(CHALLENGE_COLORS[challenge.type] ?? '#94a3b8');
  let wasExtracted = $derived(provenance?.source === 'extracted');
</script>

<div
  class="border-l-3 pl-3 py-2 mt-2"
  style="border-color: {borderColor};"
>
  <div class="flex items-center gap-2 mb-1 flex-wrap">
    <span
      class="text-[10px] mono uppercase px-1.5 py-0.5 rounded"
      style="color: {borderColor}; background: {borderColor}15;"
    >
      {challenge.type} challenge
    </span>
    {#if wasExtracted}
      <span
        class="text-[10px] mono uppercase px-1.5 py-0.5 rounded text-[#8b5cf6] bg-[#8b5cf6]/10 border border-[#8b5cf6]/30"
        title="Extracted from the bot's prose by MiniMax with source-quote verification. Raw text is preserved above."
      >
        extracted
      </span>
    {/if}
  </div>
  <p class="text-xs text-[var(--text-secondary)] mb-1">
    <span class="text-[var(--text-muted)]">Claim targeted:</span> {challenge.claim_targeted}
  </p>
  <p class="text-xs text-[var(--text-secondary)]">
    <span class="text-[var(--text-muted)]">Counter-evidence:</span> {challenge.counter_evidence}
  </p>

  {#if wasExtracted && provenance?.quote}
    <p class="text-[11px] text-[var(--text-muted)] mt-1.5 italic">
      <span class="mono uppercase tracking-wider text-[10px] not-italic">Source quote:</span>
      &ldquo;{provenance.quote}&rdquo;
    </p>
  {/if}

  {#if validationReasoning}
    <button
      onclick={() => (showReasoning = !showReasoning)}
      class="text-[10px] mono text-[var(--text-muted)] hover:text-[var(--text-secondary)] mt-1.5 transition-colors"
    >
      {showReasoning ? 'Hide' : 'Show'} validation reasoning
    </button>
    {#if showReasoning}
      <p class="text-[11px] text-[var(--text-muted)] mt-1 italic">{validationReasoning}</p>
    {/if}
  {/if}
</div>
