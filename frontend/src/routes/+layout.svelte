<script lang="ts">
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { getClerk, isSignedIn } from '$lib/auth/clerk';
  import { me, refreshMe } from '$lib/stores/me';

  let { children } = $props();
  let ready = $state(false);

  onMount(async () => {
    const path = $page.url.pathname;
    // Sign-in page bypasses the guard.
    if (path === '/sign-in') {
      ready = true;
      return;
    }
    try {
      await getClerk();
      if (!(await isSignedIn())) {
        await goto('/sign-in');
        return;
      }
      await refreshMe();
      ready = true;
    } catch (e) {
      // Clerk failed to load (e.g. publishable key missing in dev).
      // Show the app in a degraded state rather than lock the user out.
      console.error('Clerk load failed', e);
      ready = true;
    }
  });
</script>

{#if $page.url.pathname === '/sign-in'}
  {@render children()}
{:else if ready}
  <div class="flex min-h-screen">
    <Sidebar currentPath={$page.url.pathname} role={$me?.role ?? 'member'} />
    <main class="ml-56 flex-1 p-8">
      {@render children()}
    </main>
  </div>
{:else}
  <div class="flex items-center justify-center min-h-screen">
    <p class="mono text-xs text-[var(--text-muted)]">Loading...</p>
  </div>
{/if}
