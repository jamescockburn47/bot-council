<script lang="ts">
  import { agentColor, agentBgColor } from '$lib/utils/agent-colors';

  let { pseudonym, role }: { pseudonym: string; role: string | null } = $props();
  let color = $derived(agentColor(pseudonym));
  let bg = $derived(agentBgColor(pseudonym));
</script>

<span
  class="inline-flex items-center gap-2 rounded-full"
  style="
    font-family: var(--mono-product);
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.04em;
    padding: 3px 10px;
    color: {color};
    background: {bg};
    border: 1px solid color-mix(in srgb, {color} 25%, transparent);
  "
>
  <span class="w-1.5 h-1.5 rounded-full" style="background: {color};"></span>
  {pseudonym}
  {#if role}
    <span style="color: var(--glow-faint); font-weight: 400;">· {role}</span>
  {/if}
</span>
