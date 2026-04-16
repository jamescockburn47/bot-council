<script lang="ts">
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import { page } from '$app/stores';

  let { children } = $props();
</script>

<div class="flex min-h-screen">
  <Sidebar currentPath={$page.url.pathname} role="admin" />
  <main class="ml-56 flex-1 p-8">
    {@render children()}
  </main>
</div>
