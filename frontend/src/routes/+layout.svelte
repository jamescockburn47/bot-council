<script lang="ts">
  import '../app.css';
  import Sidebar from '$lib/components/Sidebar.svelte';
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

  // Read the current path from window.location rather than subscribing to
  // `$page`. Reactive `$page` has repeatedly been the source of the Svelte 5
  // "Cannot read properties of null (reading 'r')" runtime crash — during
  // early static-adapter hydration the page store's value is null and the
  // compiled store-as-signal access crashes in the runtime.
  //
  // `afterNavigate` from `$app/navigation` was tried first (as a callback
  // hook, not a reactive store) but in practice our $state update inside its
  // callback didn't always propagate to the layout template — after clicking
  // a `goto('/debates')` button from the public landing page the layout
  // stayed on the public branch until a full refresh. Instead, listen to
  // history events directly: `popstate` for back/forward, and intercepted
  // `pushState` / `replaceState` for SvelteKit's client-router navigations.
  // That keeps `currentPath` in sync with `window.location.pathname`
  // regardless of which code path triggers the change.
  function readPath(): string {
    if (typeof window === 'undefined') return '';
    return window.location.pathname;
  }

  let currentPath = $state<string>(readPath());

  $effect(() => {
    if (typeof window === 'undefined') return;

    const sync = () => {
      currentPath = readPath();
    };
    sync();

    const origPush = history.pushState;
    const origReplace = history.replaceState;
    history.pushState = function (...args) {
      const result = origPush.apply(this, args as Parameters<typeof origPush>);
      sync();
      return result;
    };
    history.replaceState = function (...args) {
      const result = origReplace.apply(this, args as Parameters<typeof origReplace>);
      sync();
      return result;
    };
    window.addEventListener('popstate', sync);
    window.addEventListener('hashchange', sync);

    return () => {
      history.pushState = origPush;
      history.replaceState = origReplace;
      window.removeEventListener('popstate', sync);
      window.removeEventListener('hashchange', sync);
    };
  });

  $effect(() => {
    if (didRunAuthBootstrap) return;
    didRunAuthBootstrap = true;

    void (async () => {
      const path = currentPath;
      const isPublic = PUBLIC_PATHS.has(path);
      console.info('[layout] onMount start, path=', path, 'public=', isPublic);

      try {
        console.info('[layout] stage=loading-clerk');
        stage = 'loading-clerk';
        await getClerk();

        console.info('[layout] stage=checking-session');
        stage = 'checking-session';
        const signedIn = await isSignedIn();

        if (!signedIn) {
          if (isPublic) {
            console.info('[layout] stage=ready (public, signed-out)');
            stage = 'ready';
            return;
          }
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
        if (isPublic) {
          console.warn('[layout] public-path auth init failed, rendering public content anyway', e);
          stage = 'ready';
          return;
        }
        console.error('[layout] auth init failed at stage', stage, e);
        fatalError = `Failed at stage "${stageLabel[stage]}": ${
          e instanceof Error ? e.message : String(e)
        }`;
      }
    })();
  });
</script>

{#if PUBLIC_PATHS.has(currentPath)}
  {@render children()}
{:else if fatalError}
  <div class="flex items-center justify-center min-h-screen flex-col gap-3 p-8" style="background: var(--night);">
    <p class="mono-label" style="color: #EF4444;">Auth initialisation failed</p>
    <p
      class="max-w-lg text-center whitespace-pre-wrap"
      style="font-family: var(--mono-product); font-size: 12px; color: var(--glow-faint); line-height: 1.6;"
    >
      {fatalError}
    </p>
    <div class="flex gap-3 mt-2">
      <a
        href="/sign-in"
        class="no-underline"
        style="font-family: var(--mono-product); font-size: 12px; color: var(--indigo-400);"
      >Go to sign-in</a>
      <button
        onclick={() => location.reload()}
        style="font-family: var(--mono-product); font-size: 12px; color: var(--indigo-400); background: none; border: none; cursor: pointer;"
      >Reload</button>
    </div>
  </div>
{:else if stage === 'ready' && $me}
  <div class="flex min-h-screen" style="background: var(--night);">
    <Sidebar currentPath={currentPath} role={$me.role} />
    <main class="ml-56 flex-1 p-8">
      {@render children()}
    </main>
  </div>
{:else}
  <div class="flex items-center justify-center min-h-screen flex-col gap-2" style="background: var(--night);">
    <p
      style="font-family: var(--mono-product); font-size: 12px; color: var(--glow-mute); letter-spacing: 0.1em;"
    >
      {stageLabel[stage]}
    </p>
    <p
      style="font-family: var(--mono-product); font-size: 10px; color: var(--glow-faint); letter-spacing: 0.2em;"
    >
      stage: {stage}
    </p>
  </div>
{/if}
