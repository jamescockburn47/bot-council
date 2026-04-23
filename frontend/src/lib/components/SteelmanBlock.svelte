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

  const accent = 'var(--copper)';
</script>

{#if hasSteelmanText}
  <div
    style="
      margin-top: 12px;
      padding: 12px 14px;
      border-left: 2px solid {accent};
      background: color-mix(in srgb, var(--copper) 6%, transparent);
      border-radius: 0 var(--r-md) var(--r-md) 0;
    "
  >
    <div class="flex items-center gap-2 flex-wrap mb-2">
      <p class="mono-label" style="color: {accent}; margin-bottom: 0;">Steelman</p>
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
      {metadata.steelman}
    </p>
    {#if wasExtracted && metadata.quote}
      <p style="font-family: var(--sans-product); font-style: italic; font-size: 12px; color: var(--glow-mute); margin-top: 8px;">
        — &ldquo;{metadata.quote}&rdquo;
      </p>
    {/if}
  </div>
{:else if wasAuthored}
  <!-- External bots articulate the steelman inside their main prose
       response above; no dedicated block needed. -->
{/if}
