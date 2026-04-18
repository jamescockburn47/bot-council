// Client-side Sentry setup for the static SvelteKit SPA.
//
// We use `@sentry/browser` (not `@sentry/sveltekit`) because this app is a
// pure CSR static build — `adapter-static` with `ssr=false; prerender=false;`.
// `@sentry/sveltekit` assumes an SSR/edge runtime and its v10 export map
// fails to resolve under vite when that runtime is absent. The browser SDK
// auto-registers window error + unhandledrejection handlers, which is all
// we need in this context.
//
// DSN is read from `$env/dynamic/public`. Empty/missing `PUBLIC_SENTRY_DSN`
// produces a safe no-op (init not called).

import * as Sentry from '@sentry/browser';
import { env } from '$env/dynamic/public';
import type { HandleClientError } from '@sveltejs/kit';
import { beforeBreadcrumb, beforeSend } from '$lib/observability/scrubber';

const dsn = env.PUBLIC_SENTRY_DSN;

if (dsn) {
  const doInit = () => {
    Sentry.init({
      dsn,
      environment: env.PUBLIC_SENTRY_ENVIRONMENT || 'prod',
      sampleRate: 1.0,
      // Performance tracing off by default — low-traffic app, not worth the
      // quota. Raise via env later if we want transaction traces.
      tracesSampleRate: 0,
      sendDefaultPii: false,
      beforeSend,
      beforeBreadcrumb,
    });
  };

  // Defer init until after the first mount microtask to avoid startup races
  // with app/bootstrap code. Trade-off: errors thrown during module evaluation
  // before this callback runs are not captured.
  if (typeof queueMicrotask === 'function') {
    queueMicrotask(doInit);
  } else {
    setTimeout(doInit, 0);
  }
}

// SvelteKit client-side error hook. Report unhandled navigation/render
// errors to Sentry. `Sentry.captureException` is a no-op when init wasn't
// called, so this is safe with or without a DSN.
export const handleError: HandleClientError = ({ error, event }) => {
  Sentry.captureException(error, {
    extra: { url: event.url.toString() },
  });
  console.error('[client] unhandled error', error, event);
};
