import { writable } from 'svelte/store';
import type { UserInfoResponse } from '$lib/types';
import { api, ApiError } from '$lib/api/client';

/** Current signed-in user identity. null = not yet loaded or unauthenticated. */
export const me = writable<UserInfoResponse | null>(null);

/** Fetch /me and populate the store. Called from the root layout after Clerk loads. */
export async function refreshMe(): Promise<void> {
  try {
    const info = await api.me();
    me.set(info);
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) {
      me.set(null);
    } else {
      me.set(null);
    }
  }
}
