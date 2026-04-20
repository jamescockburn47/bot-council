import { goto } from '$app/navigation';
import { getSessionToken } from '$lib/auth/clerk';
import type {
  AdminEntry,
  BotResponse,
  CreateBotRequest,
  CreateDebateRequest,
  DebateResponse,
  SeenUserEntry,
  SynthesisResponse,
  TranscriptResponse,
  UserInfoResponse,
} from '$lib/types';

// Single-origin architecture: Axum on EVO serves both the API (under /api/*)
// and the static frontend (under /*). Frontend always talks to `/api` relative
// to the current origin — no cross-origin calls, no build-time env var needed.
const BASE_URL = '/api';

class ApiError extends Error {
  constructor(
    public status: number,
    public body: unknown,
  ) {
    super(`API error ${status}`);
  }
}

// Default read timeout — covers nearly all GETs in the sub-second range plus
// network jitter. Short enough to surface a truly hung request.
const DEFAULT_TIMEOUT_MS = 15_000;

// Some mutations (notably POST /debates) run a synchronous preflight that
// smoke-tests every selected bot's HTTP endpoint before returning. Observed
// in prod at ~136s for a five-bot debate creation; even though the smoke
// tests are parallel via join_all, each call waits on a full LLM round-trip
// on the bot's configured endpoint. Six minutes gives plenty of margin
// without being an absurd hang detector.
const MUTATION_TIMEOUT_MS = 360_000;

function timeoutForMethod(method: string | undefined): number {
  const m = (method ?? 'GET').toUpperCase();
  if (m === 'POST' || m === 'PATCH' || m === 'DELETE' || m === 'PUT') {
    return MUTATION_TIMEOUT_MS;
  }
  return DEFAULT_TIMEOUT_MS;
}

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...((options.headers as Record<string, string>) ?? {}),
  };
  try {
    const token = await getSessionToken();
    if (token) headers['Authorization'] = `Bearer ${token}`;
  } catch {
    // Clerk not yet loaded / not configured — fall through without auth.
  }
  const controller = new AbortController();
  const timeoutMs = timeoutForMethod(options.method);
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  const res = await fetch(`${BASE_URL}${path}`, { ...options, headers, signal: controller.signal })
    .finally(() => clearTimeout(timeout));
  if (res.status === 401) {
    await goto('/sign-in');
    throw new ApiError(401, null);
  }
  if (!res.ok) {
    const body = await res.json().catch(() => null);
    throw new ApiError(res.status, body);
  }
  if (res.status === 204) return undefined as T;
  return res.json();
}

export const api = {
  me: () => request<UserInfoResponse>('/me'),

  debates: {
    list: (params?: { status?: string; limit?: number }) => {
      const sp = new URLSearchParams();
      if (params?.status) sp.set('status', params.status);
      if (params?.limit) sp.set('limit', String(params.limit));
      const qs = sp.toString();
      return request<DebateResponse[]>(`/debates${qs ? `?${qs}` : ''}`);
    },
    get: (id: string) => request<DebateResponse>(`/debates/${id}`),
    create: (req: CreateDebateRequest) =>
      request<DebateResponse>('/debates', {
        method: 'POST',
        body: JSON.stringify(req),
      }),
    transcript: (id: string) => request<TranscriptResponse>(`/debates/${id}/transcript`),
    synthesis: (id: string) => request<SynthesisResponse>(`/debates/${id}/synthesis`),
  },

  bots: {
    list: () => request<BotResponse[]>('/bots'),
    create: (req: CreateBotRequest) =>
      request<BotResponse>('/bots', {
        method: 'POST',
        body: JSON.stringify(req),
      }),
    approve: (id: string) => request<BotResponse>(`/bots/${id}/approve`, { method: 'PATCH' }),
    reject: (id: string, reason: string) =>
      request<BotResponse>(`/bots/${id}/reject`, {
        method: 'PATCH',
        body: JSON.stringify({ reason }),
      }),
    deactivate: (id: string) => request<void>(`/bots/${id}/deactivate`, { method: 'PATCH' }),
    reactivate: (id: string) => request<void>(`/bots/${id}/reactivate`, { method: 'PATCH' }),
    mySubmissions: () => request<BotResponse[]>('/bots/my-submissions'),
  },

  admins: {
    list: () => request<AdminEntry[]>('/admins'),
    add: (user_id: string) =>
      request<AdminEntry>('/admins', {
        method: 'POST',
        body: JSON.stringify({ user_id }),
      }),
    remove: (user_id: string) =>
      request<void>(`/admins/${encodeURIComponent(user_id)}`, { method: 'DELETE' }),
  },

  users: {
    list: () => request<SeenUserEntry[]>('/users'),
  },
};

/**
 * Build the SSE stream URL with an optional auth token appended as a query
 * parameter. EventSource cannot set `Authorization` headers, so authenticated
 * stream consumers must embed the token in the URL. Prefer the Clerk session
 * token when available; it's short-lived and scoped.
 */
export function debateStreamUrl(debateId: string, token?: string | null): string {
  const base = `${BASE_URL}/debates/${debateId}/stream`;
  return token ? `${base}?token=${encodeURIComponent(token)}` : base;
}

export { ApiError };
