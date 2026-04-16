<script lang="ts">
  import { onMount } from 'svelte';
  import { getClerk } from '$lib/auth/clerk';

  let container: HTMLDivElement;
  let error = $state<string | null>(null);

  onMount(async () => {
    try {
      const clerk = await getClerk();
      clerk.mountSignIn(container, {
        afterSignInUrl: '/',
        afterSignUpUrl: '/',
      });
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load Clerk';
    }
  });
</script>

<div class="flex items-center justify-center min-h-screen bg-[var(--bg)]">
  {#if error}
    <p class="text-red-400 mono text-sm">{error}</p>
  {:else}
    <div bind:this={container}></div>
  {/if}
</div>
