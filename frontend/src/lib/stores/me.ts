import { writable } from 'svelte/store';
import type { UserInfoResponse } from '$lib/types';
import { api, ApiError } from '$lib/api/client';

/** Current signed-in user identity. null = not yet loaded or unauthenticated. */
export const me = writable<UserInfoResponse | null>(null);

/** Fetch /me and populate the store. Throws on failure so callers can react. */
export async function refreshMe(): Promise<UserInfoResponse> {
  try {
    const info = await api.me();
    me.set(info);
    return info;
  } catch (e) {
    me.set(null);
    if (e instanceof ApiError) {
      throw new Error(`/me returned ${e.status}: ${JSON.stringify(e.body)}`);
    }
    throw e;
  }
}
