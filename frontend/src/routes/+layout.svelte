<script lang="ts">
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
  import { page } from '$app/stores';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { env } from '$env/dynamic/public';
  import { getClerk, isSignedIn } from '$lib/auth/clerk';
  import { me, refreshMe } from '$lib/stores/me';

  let { children } = $props();

  // Paths that render without requiring an authenticated session. The landing
  // page and the docs-style pages must not be gated on Clerk — they're public
  // collateral and used to be completely unreachable when Clerk hung or
  // returned a bad session. /sign-in is also public (it IS the gate).
  const PUBLIC_PATHS = new Set(['/', '/sign-in', '/security', '/how-it-works']);
  const PUBLIC_PREFIXES = ['/bots/guide', '/bots/criteria'];

  function isPublicPath(path: string): boolean {
    if (PUBLIC_PATHS.has(path)) return true;
    return PUBLIC_PREFIXES.some((p) => path === p || path.startsWith(p + '/'));
  }

  type Stage =
    | 'init'
    | 'loading-clerk'
    | 'checking-session'
    | 'redirecting-sign-in'
    | 'fetching-me'
    | 'ready';
  let stage = $state<Stage>('init');
  let fatalError = $state<string | null>(null);

  const stageLabel: Record<Stage, string> = {
    init: 'Starting…',
    'loading-clerk': 'Loading Clerk…',
    'checking-session': 'Checking session…',
    'redirecting-sign-in': 'Redirecting to sign-in…',
    'fetching-me': 'Fetching profile…',
    ready: 'Ready',
  };

  // Safety net: if auth doesn't complete in 20 s, show the fatal-error panel
  // rather than hanging forever at "Starting…". The specific stage tells us
  // which step got stuck.
  function scheduleAuthTimeout(): number {
    return window.setTimeout(() => {
      if (stage !== 'ready') {
        const msg = `Auth timed out at stage "${stageLabel[stage]}" (after 20 s)`;
        console.error('[layout]', msg);
        fatalError = msg;
      }
    }, 20_000);
  }

  onMount(async () => {
    const path = $page.url.pathname;
    console.info('[layout] onMount start, path=', path);

    // Public paths: render children immediately, optionally refresh `me` in
    // the background if a session already exists (so the sidebar still
    // populates for signed-in users who land on a public page).
    if (isPublicPath(path)) {
      console.info('[layout] public path — skipping auth gate');
      stage = 'ready';
      // Best-effort background refresh of $me; don't block UI on it.
      refreshMe().catch((e) => console.warn('[layout] background refreshMe failed', e));
      return;
    }

    if (!env.PUBLIC_CLERK_PUBLISHABLE_KEY) {
      fatalError = 'PUBLIC_CLERK_PUBLISHABLE_KEY is not set in the deployed bundle.';
      return;
    }
    if (!env.PUBLIC_API_URL) {
      fatalError = 'PUBLIC_API_URL is not set in the deployed bundle.';
      return;
    }

    const timeoutHandle = scheduleAuthTimeout();

    try {
      stage = 'loading-clerk';
      console.info('[layout] stage=loading-clerk');
      await getClerk();

      stage = 'checking-session';
      console.info('[layout] stage=checking-session');
      const signedIn = await isSignedIn();
      if (!signedIn) {
        stage = 'redirecting-sign-in';
        console.info('[layout] stage=redirecting-sign-in');
        await goto('/sign-in');
        window.clearTimeout(timeoutHandle);
        return;
      }

      stage = 'fetching-me';
      console.info('[layout] stage=fetching-me');
      await refreshMe();

      stage = 'ready';
      console.info('[layout] stage=ready');
    } catch (e) {
      console.error('[layout] auth init failed at stage', stage, e);
      fatalError = `Failed at stage "${stageLabel[stage]}": ${
        e instanceof Error ? e.message : String(e)
      }`;
    } finally {
      window.clearTimeout(timeoutHandle);
    }
  });
</script>

{#if isPublicPath($page.url.pathname) && !fatalError}
  {#if $me}
    <div class="flex min-h-screen">
      <Sidebar currentPath={$page.url.pathname} role={$me.role} />
      <main class="ml-56 flex-1 p-8">
        {@render children()}
      </main>
    </div>
  {:else}
    <main class="p-8">
      {@render children()}
    </main>
  {/if}
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
