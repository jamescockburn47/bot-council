<script lang="ts">
  import { CHALLENGE_COLORS } from '$lib/utils/agent-colors';
  import type { ChallengeData, ExtractionProvenance } from '$lib/types';

  let { challenge, validationReasoning = null, provenance = null }: {
    challenge: ChallengeData;
    validationReasoning?: string | null;
    provenance?: ExtractionProvenance | null;
  } = $props();

  let showReasoning = $state(false);
  let accent = $derived(CHALLENGE_COLORS[challenge.type] ?? '#94a3b8');
  let wasExtracted = $derived(provenance?.source === 'extracted');
</script>

<div
  style="
    margin-top: 12px;
    padding: 12px 14px;
    border-left: 2px solid {accent};
    background: color-mix(in srgb, {accent} 6%, transparent);
    border-radius: 0 var(--r-md) var(--r-md) 0;
  "
>
  <div class="flex items-center gap-2 mb-2 flex-wrap">
    <p class="mono-label" style="color: {accent}; margin-bottom: 0;">
      {challenge.type} challenge
    </p>
    {#if wasExtracted}
      <span
        class="mono-label"
        style="padding: 2px 6px; border-radius: var(--r-sm); color: var(--indigo-400); background: rgba(99,102,241,0.10); border: 1px solid rgba(99,102,241,0.25);"
        title="Extracted from the bot's prose by MiniMax with source-quote verification. Raw text is preserved above."
      >
        extracted
      </span>
    {/if}
  </div>

  <p style="font-family: var(--sans-product); font-size: 14px; line-height: 1.6; color: var(--glow-dim); white-space: pre-wrap;">
    <span style="color: var(--glow-mute);">Claim targeted:</span> {challenge.claim_targeted}
  </p>
  <p style="font-family: var(--sans-product); font-size: 14px; line-height: 1.6; color: var(--glow-dim); white-space: pre-wrap; margin-top: 4px;">
    <span style="color: var(--glow-mute);">Counter-evidence:</span> {challenge.counter_evidence}
  </p>

  {#if wasExtracted && provenance?.quote}
    <p style="font-family: var(--sans-product); font-style: italic; font-size: 12px; color: var(--glow-mute); margin-top: 8px;">
      — &ldquo;{provenance.quote}&rdquo;
    </p>
  {/if}

  {#if validationReasoning}
    <button
      onclick={() => (showReasoning = !showReasoning)}
      class="mono-label transition-colors"
      style="margin-top: 6px; cursor: pointer; background: none; border: none; padding: 0;"
      onmouseenter={(e) => (e.currentTarget.style.color = 'var(--glow-dim)')}
      onmouseleave={(e) => (e.currentTarget.style.color = '')}
    >
      {showReasoning ? 'Hide' : 'Show'} validation reasoning
    </button>
    {#if showReasoning}
      <p style="font-family: var(--sans-product); font-style: italic; font-size: 12px; color: var(--glow-mute); margin-top: 4px;">{validationReasoning}</p>
    {/if}
  {/if}
</div>
