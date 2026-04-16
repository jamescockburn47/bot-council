export const AGENT_COLORS: Record<string, string> = {
  'Agent A': '#f472b6',
  'Agent B': '#34d399',
  'Agent C': '#60a5fa',
  'Agent D': '#f59e0b',
  'Agent E': '#8b5cf6',
};

export const AGENT_BG_COLORS: Record<string, string> = {
  'Agent A': 'rgba(244,114,182,0.1)',
  'Agent B': 'rgba(52,211,153,0.1)',
  'Agent C': 'rgba(96,165,250,0.1)',
  'Agent D': 'rgba(245,158,11,0.1)',
  'Agent E': 'rgba(139,92,246,0.1)',
};

export function agentColor(pseudonym: string): string {
  return AGENT_COLORS[pseudonym] ?? '#94a3b8';
}

export function agentBgColor(pseudonym: string): string {
  return AGENT_BG_COLORS[pseudonym] ?? 'rgba(148,163,184,0.1)';
}

export const STATUS_COLORS: Record<string, string> = {
  running: '#8b5cf6',
  complete: '#22c55e',
  cancelled: '#ef4444',
  created: '#94a3b8',
  dispatching: '#8b5cf6',
  failed: '#ef4444',
  pending: '#f59e0b',
  active: '#22c55e',
  inactive: '#94a3b8',
  rejected: '#ef4444',
};

export const CHALLENGE_COLORS: Record<string, string> = {
  factual: '#ef4444',
  logical: '#f59e0b',
  premise: '#8b5cf6',
};
