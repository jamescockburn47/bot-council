import { env } from '$env/dynamic/public';
import type {
  BotResponse,
  CreateBotRequest,
  CreateDebateRequest,
  DebateResponse,
  SynthesisResponse,
  TranscriptResponse,
  UserInfoResponse,
} from '$lib/types';

const BASE_URL = env.PUBLIC_API_URL;

class ApiError extends Error {
  constructor(
    public status: number,
    public body: unknown,
  ) {
    super(`API error ${status}`);
  }
}

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...((options.headers as Record<string, string>) ?? {}),
  };
  // Auth token will be added when Clerk is wired up
  const res = await fetch(`${BASE_URL}${path}`, { ...options, headers });
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
    reject: (id: string) => request<BotResponse>(`/bots/${id}/reject`, { method: 'PATCH' }),
    deactivate: (id: string) => request<void>(`/bots/${id}/deactivate`, { method: 'PATCH' }),
    reactivate: (id: string) => request<void>(`/bots/${id}/reactivate`, { method: 'PATCH' }),
    mySubmissions: () => request<BotResponse[]>('/bots/my-submissions'),
  },
};

/// Build the SSE stream URL. Accepts an optional auth token which is passed
/// as a query parameter (EventSource cannot set Authorization headers).
/// Token will come from Clerk session once auth is fully wired.
export function debateStreamUrl(debateId: string, token?: string): string {
  const base = `${BASE_URL}/debates/${debateId}/stream`;
  return token ? `${base}?token=${encodeURIComponent(token)}` : base;
}

export { ApiError };
