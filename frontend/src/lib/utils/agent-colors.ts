/*
 * Agent and status colour palette for the Terminal surface.
 *
 * The 5-agent palette is drawn from the LegalQuants brand tokens so every
 * agent hue reads as part of the same system (indigo → cyan → amber →
 * copper-light → violet). ArgumentMap3D.svelte consumes AGENT_COLORS
 * directly for 3D node tinting, so changing values here ripples through
 * the outcome view automatically.
 *
 * DO NOT introduce new hex literals. Add tokens to tokens.css first and
 * reference them by hex string here (CSS vars cannot be evaluated from
 * .ts source).
 */

export const AGENT_COLORS: Record<string, string> = {
  'Agent A': '#818CF8', // indigo-400
  'Agent B': '#10B981', // cat-llm-dark
  'Agent C': '#06B6D4', // cat-agent-dark
  'Agent D': '#F59E0B', // cat-vibe-dark
  'Agent E': '#A78BFA', // violet-400
};

export const AGENT_BG_COLORS: Record<string, string> = {
  'Agent A': 'rgba(129,140,248,0.10)',
  'Agent B': 'rgba(16,185,129,0.10)',
  'Agent C': 'rgba(6,182,212,0.10)',
  'Agent D': 'rgba(245,158,11,0.10)',
  'Agent E': 'rgba(167,139,250,0.10)',
};

export function agentColor(pseudonym: string): string {
  return AGENT_COLORS[pseudonym] ?? '#8888A0'; // glow-mute
}

export function agentBgColor(pseudonym: string): string {
  return AGENT_BG_COLORS[pseudonym] ?? 'rgba(136,136,160,0.10)';
}

export const STATUS_COLORS: Record<string, string> = {
  running:     '#6366F1', // indigo-500
  complete:    '#10B981',
  cancelled:   '#EF4444',
  created:     '#8888A0', // glow-mute
  dispatching: '#6366F1',
  failed:      '#EF4444',
  pending:     '#9A3412', // copper — intentional: "waiting on human" reads as warm flag
  active:      '#10B981',
  inactive:    '#8888A0',
  rejected:    '#EF4444',
};

export const CHALLENGE_COLORS: Record<string, string> = {
  factual: '#EF4444',
  logical: '#F59E0B',
  premise: '#A78BFA', // violet-400
};
