<script lang="ts">
  import { STATUS_COLORS } from '$lib/utils/agent-colors';

  let { status }: { status: string } = $props();
  let color = $derived(STATUS_COLORS[status.toLowerCase()] ?? '#8888A0');
  let label = $derived(status.charAt(0).toUpperCase() + status.slice(1));
</script>

<span
  class="inline-flex items-center gap-1.5 rounded-full"
  style="
    font-family: var(--mono-product);
    font-size: 10px;
    letter-spacing: 0.08em;
    padding: 3px 10px;
    color: {color};
    background: color-mix(in srgb, {color} 12%, transparent);
    border: 1px solid color-mix(in srgb, {color} 30%, transparent);
  "
>
  <span class="w-1.5 h-1.5 rounded-full" style="background: {color};"></span>
  {label}
</span>
