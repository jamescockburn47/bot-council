# Svelte 5 + SvelteKit adapter-static: `onDestroy` compiled against SSR-context in client-only build

Drafted 2026-04-21 after PRs #70–#75 chased it through multiple near-misses.
File upstream at https://github.com/sveltejs/kit/issues/new when confirmed
against a fresh repro.

## Summary

In a CSR-only SvelteKit app (`adapter-static` with `ssr = false; prerender = false`),
`import { onDestroy } from 'svelte'` is bundled to a helper whose body is

```js
function onDestroy_helper(e) { currentSsrContext.r.on_destroy(e) }
```

where `currentSsrContext` is the module-local context state of SvelteKit's
server-side renderer. That context is never set on the client, so it stays
`null`, and the first component that registers an `onDestroy` crashes with

```
TypeError: Cannot read properties of null (reading 'r')
```

## Our environment

- `svelte@5.55.4`
- `@sveltejs/kit@2.57.1`
- `@sveltejs/adapter-static@3`
- `@sveltejs/vite-plugin-svelte@4`
- `vite@6`
- Single `svelte` install confirmed via `npm ls svelte` (deduped across all consumers).
- `svelte.config.js`: `adapter: adapter({ pages: 'build', assets: 'build', fallback: 'index.html' })`.
- Root `+layout.ts` exports `prerender = false`. `ssr` defaulted (not explicitly set).

## Evidence from the built bundle

Two chunks end up in `frontend/build/_app/immutable/chunks/`:

- `CkH79pZK.js` (~27 KB). Contains the SvelteKit **server renderer**: class `v`
  with methods like `head`, `async_block`, `child`, `boundary`, and an
  `on_destroy(e)` that pushes into the renderer instance. Exports include
  `A as c` where `A` is the module-local `var A = null` current-component-context
  set by internal `T(r) { A = r }` pushes during SSR rendering. No code path
  in the emitted client code ever calls `T`.

- `CDDrkIau.js` (~40 KB). Contains the client `onDestroy` helper:

  ```js
  import { c as Mt, B as A, _ as zt, w as Ge } from "./CkH79pZK.js";
  function Qn(e) { Mt.r.on_destroy(e) }
  // …
  export { Qn as o, … };
  ```

  User code `import { onDestroy } from 'svelte'` is rewritten to use `Qn` via
  an alias. Since `Mt` is the `A` export from the SSR renderer chunk and `A`
  is `null` on the client, `Mt.r` throws.

The two chunks have no reverse linkage — `Wx_jKA78.js` (which does contain
the client-side `push`/`pop` of `x`, analogous to the server context) does
not import from `CkH79pZK.js`, so the client push never updates `A`.

## Minimum observed reproduction in this project

Any route that mounts a component whose `<script>` block contains
`import { onDestroy } from 'svelte'` and calls `onDestroy(...)` during
component init. Two examples:

- `frontend/src/lib/components/outcome/ArgumentMap.svelte` (calls
  `onDestroy(() => handle?.stop())` to tear down a d3-force simulation)
- `frontend/src/lib/components/outcome/ReplaySlider.svelte` (calls
  `onDestroy(() => clearInterval(playTimer))`)

Workaround: replace `onDestroy(fn)` with `$effect(() => () => fn())`. Effect
return cleanup is routed through the client runtime, not the SSR context
shim. See PR #73.

## A related, possibly same-root-cause bug

The page store from `$app/stores` triggers `Cannot read properties of null
(reading 'r')` in the same bundle on first hydration when read reactively
(`$page.url.pathname`). The store-as-signal compiler path appears to cross
the same SSR/CSR boundary. Workaround: read `window.location` + refresh via
`afterNavigate`. See PR #72.

## What a clean repro probably needs

- `npm create svelte@latest` with TypeScript + adapter-static
- Set `export const ssr = false` in `+layout.ts`
- Add a single component with `onDestroy(() => console.log('teardown'))`
- Mount it from the root route
- `npm run build && npm run preview`
- Open in browser → expect the crash on first component mount

If that reproduces, the issue is in `@sveltejs/kit`'s client runtime import
resolution. If it doesn't, something specific to this project's dependency
graph is forcing `onDestroy`'s chunk to bind against the server renderer
module instead of the client runtime module.

## Guardrails in place in this repo

`.github/workflows/ci.yml` forbids `onDestroy` from `'svelte'` and `page`
from `'$app/stores'` in `frontend/src/**/*.{svelte,ts,js}`. See PR #75.
Lift the guard once upstream is fixed and a clean build in this repo
doesn't exhibit the bundling pattern above.
