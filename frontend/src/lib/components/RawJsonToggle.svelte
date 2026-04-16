<script lang="ts">
  let { data }: { data: unknown } = $props();
  let open = $state(false);
</script>

<div class="mt-2">
  <button
    onclick={() => (open = !open)}
    class="text-[10px] mono text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
  >
    {open ? 'Hide' : 'View'} raw JSON
  </button>
  {#if open}
    <pre
      class="mt-2 p-3 bg-[var(--bg)] border border-[var(--border)] rounded text-[11px] mono text-[var(--text-secondary)] overflow-x-auto max-h-96 overflow-y-auto"
    >{JSON.stringify(data, null, 2)}</pre>
  {/if}
</div>
