<script lang="ts">
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { getClerk, isSignedIn } from '$lib/auth/clerk';
  import { me, refreshMe } from '$lib/stores/me';

  let { children } = $props();

  onMount(async () => {
    const path = $page.url.pathname;
    if (path === '/sign-in') return;
    try {
      await getClerk();
      if (!(await isSignedIn())) {
        await goto('/sign-in');
        return;
      }
      await refreshMe();
    } catch (e) {
      console.error('Clerk load failed', e);
      await goto('/sign-in');
    }
  });
</script>

{#if $page.url.pathname === '/sign-in'}
  {@render children()}
{:else}
  <div class="flex min-h-screen">
    <Sidebar currentPath={$page.url.pathname} role={$me?.role ?? 'member'} />
    <main class="ml-56 flex-1 p-8">
      {@render children()}
    </main>
  </div>
{/if}
