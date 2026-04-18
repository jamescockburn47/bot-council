<script lang="ts">
  import { getClerk } from '$lib/auth/clerk';

  let container = $state<HTMLDivElement | null>(null);
  let error = $state<string | null>(null);
  let mounted = $state(false);
  let didMountSignIn = false;

  $effect(() => {
    if (didMountSignIn || !container) return;
    didMountSignIn = true;

    void (async () => {
      try {
        const clerk = await getClerk();
        clerk.mountSignIn(container, {
          fallbackRedirectUrl: '/',
          signUpFallbackRedirectUrl: '/',
        });
        mounted = true;
      } catch (e) {
        error = e instanceof Error ? e.message : 'Failed to load Clerk';
      }
    })();
  });
</script>

<div class="flex flex-col items-center justify-center min-h-screen bg-[var(--bg)] p-4">
  <h1 class="mono text-2xl font-bold mb-6">LQ Council</h1>
  {#if error}
    <div class="text-center max-w-md">
      <p class="text-red-400 mono text-sm mb-4">Sign-in unavailable: {error}</p>
      <button
        onclick={() => location.reload()}
        class="px-4 py-2 bg-[#8b5cf6] text-white rounded-lg mono text-sm"
      >
        Reload
      </button>
    </div>
  {:else}
    <div bind:this={container}></div>
    {#if !mounted}
      <p class="mono text-xs text-[var(--text-muted)] mt-4">Loading sign-in…</p>
    {/if}
  {/if}
</div>
