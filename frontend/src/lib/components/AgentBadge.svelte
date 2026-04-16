<script lang="ts">
  import { agentColor } from '$lib/utils/agent-colors';

  let { pseudonym, role = null }: { pseudonym: string; role?: string | null } = $props();
  let color = $derived(agentColor(pseudonym));
  let letter = $derived(pseudonym.replace('Agent ', ''));
</script>

<span class="inline-flex items-center gap-1.5">
  <span
    class="w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold mono"
    style="color: {color}; background: {color}20;">{letter}</span
  >
  <span class="mono text-xs" style="color: {color};">{pseudonym}</span>
  {#if role}<span class="text-[10px] text-[var(--text-muted)]">{role}</span>{/if}
</span>
