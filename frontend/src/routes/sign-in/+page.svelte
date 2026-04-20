<script lang="ts">
  import { goto } from '$app/navigation';
  import { getClerk } from '$lib/auth/clerk';
  import { isSignedIn } from '$lib/auth/clerk';

  let error = $state<string | null>(null);
  let didStartRedirect = false;

  $effect(() => {
    if (didStartRedirect) return;
    didStartRedirect = true;

    void (async () => {
      try {
        if (await isSignedIn()) {
          await goto('/');
          return;
        }

        const clerk = await getClerk();
        await clerk.redirectToSignIn({
          redirectUrl: '/',
          signInFallbackRedirectUrl: '/',
          signUpFallbackRedirectUrl: '/'
        });
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
    <p class="mono text-xs text-[var(--text-muted)] mt-4">Redirecting to sign-in…</p>
  {/if}
</div>
