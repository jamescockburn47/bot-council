<script lang="ts">
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import { page } from '$app/stores';
  import { api } from '$lib/api/client';
  import { onMount } from 'svelte';

  let { children } = $props();
  let role = $state('member'); // default to least privilege, promote on auth

  onMount(async () => {
    try {
      const me = await api.me();
      role = me.role;
    } catch {
      // Not authenticated or API unreachable — keep default
      role = 'member';
    }
  });
</script>

<div class="flex min-h-screen">
  <Sidebar currentPath={$page.url.pathname} {role} />
  <main class="ml-56 flex-1 p-8">
    {@render children()}
  </main>
</div>
