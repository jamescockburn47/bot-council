<script lang="ts">
  import type { PositionChangeData } from '$lib/types';

  let { change }: { change: PositionChangeData } = $props();
</script>

{#if change.changed}
  <div class="border-l-3 border-amber-500/60 pl-3 py-2 mt-2">
    <span class="text-[10px] mono uppercase text-amber-400 bg-amber-400/10 px-1.5 py-0.5 rounded">
      Position changed
    </span>
    <div class="mt-1.5 text-xs text-[var(--text-secondary)]">
      <p><span class="text-[var(--text-muted)]">From:</span> {change.from_summary}</p>
      <p class="mt-0.5"><span class="text-[var(--text-muted)]">To:</span> {change.to_summary}</p>
      <p class="mt-0.5"><span class="text-[var(--text-muted)]">Reason:</span> {change.reason}</p>
    </div>
  </div>
{:else}
  <div class="pl-3 py-1 mt-2">
    <span class="text-[10px] mono text-[var(--text-muted)]">
      Position held &mdash; {change.reason}
    </span>
  </div>
{/if}
