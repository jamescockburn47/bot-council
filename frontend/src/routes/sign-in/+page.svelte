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

<div style="display: flex; flex-direction: column; align-items: center; justify-content: center; min-height: 100vh; background: var(--night); padding: 16px;">
  <div style="text-align: center; margin-bottom: 32px;">
    <h1 style="font-family: var(--sans-product); font-weight: 800; font-size: 28px; color: var(--glow-txt); margin: 0 0 8px;">
      LQ Council
    </h1>
    <p class="tm-eyebrow" style="color: var(--indigo-400);">Sign in</p>
  </div>

  {#if error}
    <div class="card-term" style="text-align: center; max-width: 28rem; padding: 24px;">
      <p class="mono-label" style="color: #FCA5A5; margin-bottom: 16px;">Sign-in unavailable: {error}</p>
      <button
        onclick={() => location.reload()}
        class="btn-indigo"
      >
        Reload
      </button>
    </div>
  {:else}
    <p class="mono-label" style="color: var(--glow-mute);">Redirecting to sign-in…</p>
  {/if}
</div>
