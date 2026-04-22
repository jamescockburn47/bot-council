<script lang="ts">
  import type { PositionChangeData, ExtractionProvenance } from '$lib/types';

  let { change, provenance = null }: {
    change: PositionChangeData;
    provenance?: ExtractionProvenance | null;
  } = $props();

  let wasExtracted = $derived(provenance?.source === 'extracted');
</script>

{#if change.changed}
  <div class="border-l-3 border-amber-500/60 pl-3 py-2 mt-2">
    <div class="flex items-center gap-2 flex-wrap">
      <span class="text-[10px] mono uppercase text-amber-400 bg-amber-400/10 px-1.5 py-0.5 rounded">
        Position changed
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
    <div class="mt-1.5 text-xs text-[var(--text-secondary)]">
      <p><span class="text-[var(--text-muted)]">From:</span> {change.from_summary}</p>
      <p class="mt-0.5"><span class="text-[var(--text-muted)]">To:</span> {change.to_summary}</p>
      <p class="mt-0.5"><span class="text-[var(--text-muted)]">Reason:</span> {change.reason}</p>
    </div>
    {#if wasExtracted && provenance?.quote}
      <p class="text-[11px] text-[var(--text-muted)] mt-1.5 italic">
        <span class="mono uppercase tracking-wider text-[10px] not-italic">Source quote:</span>
        &ldquo;{provenance.quote}&rdquo;
      </p>
    {/if}
  </div>
{:else}
  <div class="pl-3 py-1 mt-2">
    <div class="flex items-center gap-2 flex-wrap">
      <span class="text-[10px] mono text-[var(--text-muted)]">
        Position held &mdash; {change.reason}
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
    {#if wasExtracted && provenance?.quote}
      <p class="text-[11px] text-[var(--text-muted)] mt-1.5 italic pl-0">
        <span class="mono uppercase tracking-wider text-[10px] not-italic">Source quote:</span>
        &ldquo;{provenance.quote}&rdquo;
      </p>
    {/if}
  </div>
{/if}
