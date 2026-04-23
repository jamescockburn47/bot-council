<script lang="ts">
  import type { PositionChangeData, ExtractionProvenance } from '$lib/types';

  let { change, provenance = null }: {
    change: PositionChangeData;
    provenance?: ExtractionProvenance | null;
  } = $props();

  let wasExtracted = $derived(provenance?.source === 'extracted');
  const accent = 'var(--indigo-400)';
</script>

{#if change.changed}
  <div
    style="
      margin-top: 12px;
      padding: 12px 14px;
      border-left: 2px solid {accent};
      background: color-mix(in srgb, var(--indigo-400) 6%, transparent);
      border-radius: 0 var(--r-md) var(--r-md) 0;
    "
  >
    <div class="flex items-center gap-2 flex-wrap mb-2">
      <p class="mono-label" style="color: {accent}; margin-bottom: 0;">Position changed</p>
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
      <span style="color: var(--glow-mute);">From:</span> {change.from_summary}
    </p>
    <p style="font-family: var(--sans-product); font-size: 14px; line-height: 1.6; color: var(--glow-dim); white-space: pre-wrap; margin-top: 2px;">
      <span style="color: var(--glow-mute);">To:</span> {change.to_summary}
    </p>
    <p style="font-family: var(--sans-product); font-size: 14px; line-height: 1.6; color: var(--glow-dim); white-space: pre-wrap; margin-top: 2px;">
      <span style="color: var(--glow-mute);">Reason:</span> {change.reason}
    </p>
    {#if wasExtracted && provenance?.quote}
      <p style="font-family: var(--sans-product); font-style: italic; font-size: 12px; color: var(--glow-mute); margin-top: 8px;">
        — &ldquo;{provenance.quote}&rdquo;
      </p>
    {/if}
  </div>
{:else}
  <div
    style="
      margin-top: 12px;
      padding: 8px 14px;
      border-left: 2px solid rgba(99,102,241,0.25);
      border-radius: 0 var(--r-md) var(--r-md) 0;
    "
  >
    <div class="flex items-center gap-2 flex-wrap">
      <span class="mono-label">
        Position held &mdash; {change.reason}
      </span>
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
    {#if wasExtracted && provenance?.quote}
      <p style="font-family: var(--sans-product); font-style: italic; font-size: 12px; color: var(--glow-mute); margin-top: 8px;">
        — &ldquo;{provenance.quote}&rdquo;
      </p>
    {/if}
  </div>
{/if}
