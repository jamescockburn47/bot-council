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
  Sentry.init({
    dsn,
    environment: env.PUBLIC_SENTRY_ENVIRONMENT || 'prod',
    sampleRate: 1.0,
    // Performance tracing off by default — low-traffic app, not worth the
    // quota. Raise via env later if we want transaction traces.
    tracesSampleRate: 0,
    sendDefaultPii: false,
    integrations: [
      // Replay captures the last ~60 s of DOM + network before any error,
      // so we get a video-like playback. Text and inputs are masked by
      // default; network payload bodies are NOT allowed (URLs + statuses
      // only) so bearer tokens in fetch headers are never stored.
      Sentry.replayIntegration({
        maskAllText: true,
        maskAllInputs: true,
        networkDetailAllowUrls: [],
      }),
    ],
    replaysSessionSampleRate: 0,
    replaysOnErrorSampleRate: 1.0,
    beforeSend,
    beforeBreadcrumb,
  });
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
