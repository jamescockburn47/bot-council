import { Clerk } from '@clerk/clerk-js';

// Single-origin architecture: config is fetched at runtime from /api/config.json
// (served by the Axum backend) rather than baked in at build time. This lets
// the same build target different environments and eliminates the need for
// Vercel build-time env vars.

interface PublicConfig {
  publishable_key: string;
  api_base: string;
  sentry_environment: string;
  release: string;
}

let clerkInstance: Clerk | null = null;
let loadPromise: Promise<Clerk> | null = null;
let configPromise: Promise<PublicConfig> | null = null;

function fetchConfig(): Promise<PublicConfig> {
  if (configPromise) return configPromise;
  configPromise = fetch('/api/config.json', { cache: 'no-store' })
    .then(async (r) => {
      if (!r.ok) throw new Error(`/api/config.json returned ${r.status}`);
      return (await r.json()) as PublicConfig;
    })
    .catch((e) => {
      // Null the cached promise so a retry can try again.
      configPromise = null;
      throw e;
    });
  return configPromise;
}

/** Lazy initialisation — returns a ready Clerk instance. */
export function getClerk(): Promise<Clerk> {
  if (clerkInstance) return Promise.resolve(clerkInstance);
  if (loadPromise) return loadPromise;
  const timeout = new Promise<never>((_, reject) =>
    setTimeout(() => reject(new Error('Clerk load timeout')), 12_000)
  );
  const loadClerk = async () => {
    const config = await fetchConfig();
    if (!config.publishable_key) {
      throw new Error('/api/config.json returned empty publishable_key');
    }
    const c = new Clerk(config.publishable_key);
    await c.load({ standardBrowser: true });
    clerkInstance = c;
    return c;
  };
  loadPromise = Promise.race([loadClerk(), timeout]);
  return loadPromise;
}

/** Return the current session JWT, or null if not signed in. */
export async function getSessionToken(): Promise<string | null> {
  const c = await getClerk();
  const session = c.session;
  if (!session) return null;
  return await session.getToken();
}

/** True once Clerk has loaded and a user is signed in. */
export async function isSignedIn(): Promise<boolean> {
  const c = await getClerk();
  return !!c.user;
}

/** Expose fetched config for modules that need sentry environment / release. */
export function getPublicConfig(): Promise<PublicConfig> {
  return fetchConfig();
}
