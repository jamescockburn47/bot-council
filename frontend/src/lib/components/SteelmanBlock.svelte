<script lang="ts">
  import type { SteelmanProvenance } from '$lib/types';

  let { metadata }: { metadata: SteelmanProvenance } = $props();

  let wasExtracted = $derived(metadata.source === 'extracted');
  let wasAuthored = $derived(metadata.source === 'authored');
  // Only render body text when we actually have a steelman string. For
  // authored external bots the steelman text lives inline in the prose
  // response above, so this block renders nothing useful. For failed
  // extractions we also skip — the raw prose is still visible above.
  let hasSteelmanText = $derived(
    wasExtracted && typeof metadata.steelman === 'string' && metadata.steelman.length > 0,
  );
</script>

{#if hasSteelmanText}
  <div class="border-l-3 border-[#8b5cf6]/60 pl-3 py-2 mt-2">
    <div class="flex items-center gap-2 flex-wrap">
      <span
        class="text-[10px] mono uppercase text-[#8b5cf6] bg-[#8b5cf6]/10 px-1.5 py-0.5 rounded"
      >
        Steelman
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
    <p class="text-xs text-[var(--text-secondary)] mt-1.5">
      {metadata.steelman}
    </p>
    {#if wasExtracted && metadata.quote}
      <p class="text-[11px] text-[var(--text-muted)] mt-1.5 italic">
        <span class="mono uppercase tracking-wider text-[10px] not-italic">Source quote:</span>
        &ldquo;{metadata.quote}&rdquo;
      </p>
    {/if}
  </div>
{:else if wasAuthored}
  <!-- External bots articulate the steelman inside their main prose
       response above; no dedicated block needed. -->
{/if}
