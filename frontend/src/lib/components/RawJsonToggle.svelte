<script lang="ts">
  let { data }: { data: unknown } = $props();
  let open = $state(false);
</script>

<div class="mt-2">
  <button
    onclick={() => (open = !open)}
    class="btn-dark-ghost"
    style="font-size: 10px; padding: 5px 12px;"
  >
    {open ? 'Hide' : 'View'} raw JSON
  </button>
  {#if open}
    <pre
      style="background: var(--night-edge); border: 1px solid var(--night-rule); border-radius: 8px; padding: 12px; font-family: var(--mono-product); font-size: 11px; color: var(--glow-dim); overflow-x: auto;"
    >{JSON.stringify(data, null, 2)}</pre>
  {/if}
</div>
