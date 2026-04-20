<script lang="ts">
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import { page } from '$app/stores';
  import { goto } from '$app/navigation';
  import { getClerk, isSignedIn } from '$lib/auth/clerk';
  import { me, refreshMe } from '$lib/stores/me';

  let { children } = $props();

  type Stage =
    | 'init'
    | 'loading-clerk'
    | 'checking-session'
    | 'redirecting-sign-in'
    | 'fetching-me'
    | 'ready';
  let stage = $state<Stage>('init');
  let fatalError = $state<string | null>(null);
  let didRunAuthBootstrap = false;

  const stageLabel: Record<Stage, string> = {
    init: 'Starting…',
    'loading-clerk': 'Loading Clerk…',
    'checking-session': 'Checking session…',
    'redirecting-sign-in': 'Redirecting to sign-in…',
    'fetching-me': 'Fetching profile…',
    ready: 'Ready',
  };

  const PUBLIC_PATHS = new Set(['/', '/sign-in']);

  $effect(() => {
    if (didRunAuthBootstrap) return;
    didRunAuthBootstrap = true;

    void (async () => {
      const path = $page.url.pathname;
      console.info('[layout] onMount start, path=', path);
      if (PUBLIC_PATHS.has(path)) {
        console.info('[layout] stage=ready (public route)');
        stage = 'ready';
        return;
      }

      try {
        console.info('[layout] stage=loading-clerk');
        stage = 'loading-clerk';
        await getClerk();

        console.info('[layout] stage=checking-session');
        stage = 'checking-session';
        const signedIn = await isSignedIn();
        if (!signedIn) {
          console.info('[layout] stage=redirecting-sign-in');
          stage = 'redirecting-sign-in';
          await goto('/sign-in');
          return;
        }

        console.info('[layout] stage=fetching-me');
        stage = 'fetching-me';
        await refreshMe();
        console.info('[layout] stage=ready');
        stage = 'ready';
      } catch (e) {
        console.error('[layout] auth init failed at stage', stage, e);
        fatalError = `Failed at stage "${stageLabel[stage]}": ${
          e instanceof Error ? e.message : String(e)
        }`;
      }
    })();
  });
</script>

{#if PUBLIC_PATHS.has($page.url.pathname)}
  {@render children()}
{:else if fatalError}
  <div class="flex items-center justify-center min-h-screen flex-col gap-3 p-8">
    <p class="mono text-sm text-red-400">Auth initialisation failed</p>
    <p class="mono text-xs text-[var(--text-muted)] max-w-lg text-center whitespace-pre-wrap">{fatalError}</p>
    <div class="flex gap-3 mt-2">
      <a href="/sign-in" class="mono text-xs text-[#8b5cf6] hover:underline">Go to sign-in</a>
      <button onclick={() => location.reload()} class="mono text-xs text-[#8b5cf6] hover:underline">Reload</button>
    </div>
  </div>
{:else if stage === 'ready' && $me}
  <div class="flex min-h-screen">
    <Sidebar currentPath={$page.url.pathname} role={$me.role} />
    <main class="ml-56 flex-1 p-8">
      {@render children()}
    </main>
  </div>
{:else}
  <div class="flex items-center justify-center min-h-screen flex-col gap-2">
    <p class="mono text-xs text-[var(--text-muted)]">{stageLabel[stage]}</p>
    <p class="mono text-[10px] text-[var(--text-muted)]/60">stage: {stage}</p>
  </div>
{/if}
