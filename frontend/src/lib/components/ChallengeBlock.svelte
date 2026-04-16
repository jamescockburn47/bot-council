<script lang="ts">
  import { CHALLENGE_COLORS } from '$lib/utils/agent-colors';
  import type { ChallengeData } from '$lib/types';

  let { challenge, validationReasoning = null }: {
    challenge: ChallengeData;
    validationReasoning?: string | null;
  } = $props();

  let showReasoning = $state(false);
  let borderColor = $derived(CHALLENGE_COLORS[challenge.type] ?? '#94a3b8');
</script>

<div
  class="border-l-3 pl-3 py-2 mt-2"
  style="border-color: {borderColor};"
>
  <div class="flex items-center gap-2 mb-1">
    <span
      class="text-[10px] mono uppercase px-1.5 py-0.5 rounded"
      style="color: {borderColor}; background: {borderColor}15;"
    >
      {challenge.type} challenge
    </span>
  </div>
  <p class="text-xs text-[var(--text-secondary)] mb-1">
    <span class="text-[var(--text-muted)]">Claim targeted:</span> {challenge.claim_targeted}
  </p>
  <p class="text-xs text-[var(--text-secondary)]">
    <span class="text-[var(--text-muted)]">Counter-evidence:</span> {challenge.counter_evidence}
  </p>

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
