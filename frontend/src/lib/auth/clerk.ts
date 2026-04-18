import { Clerk } from '@clerk/clerk-js';
import { env } from '$env/dynamic/public';

let clerkInstance: Clerk | null = null;
let loadPromise: Promise<Clerk> | null = null;

/** Lazy initialisation — returns a ready Clerk instance. */
export function getClerk(): Promise<Clerk> {
  if (clerkInstance) return Promise.resolve(clerkInstance);
  if (loadPromise) return loadPromise;
  const key = env.PUBLIC_CLERK_PUBLISHABLE_KEY;
  if (!key) {
    return Promise.reject(new Error('PUBLIC_CLERK_PUBLISHABLE_KEY is not set'));
  }
  const c = new Clerk(key);
  const timeout = new Promise<never>((_, reject) =>
    setTimeout(() => reject(new Error('Clerk load timeout')), 12_000)
  );
  // Ensure Clerk UI components (e.g. mountSignIn) are available after load.
  loadPromise = Promise.race([c.load({ standardBrowser: true }), timeout]).then(() => {
    clerkInstance = c;
    return c;
  });
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
