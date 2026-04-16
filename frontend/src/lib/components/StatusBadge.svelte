<script lang="ts">
  import { STATUS_COLORS } from '$lib/utils/agent-colors';

  let { status }: { status: string } = $props();
  let color = $derived(STATUS_COLORS[status.toLowerCase()] ?? '#94a3b8');
  let label = $derived(status.charAt(0).toUpperCase() + status.slice(1));
</script>

<span
  class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs mono"
  style="color: {color}; background: {color}15; border: 1px solid {color}30;"
>
  <span class="w-1.5 h-1.5 rounded-full" style="background: {color};"></span>
  {label}
</span>
