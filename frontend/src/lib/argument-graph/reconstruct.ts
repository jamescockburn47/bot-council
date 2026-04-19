import type { SynthesisData, TranscriptResponse } from '$lib/types';
import type { GraphState } from './types';
import { deriveGraph } from './derive';

/**
 * Levenshtein similarity ratio in [0, 1]. 1 = identical, 0 = nothing in common.
 * Dynamic programming O(mn); sufficient for a handful of short claim strings.
 */
function levRatio(a: string, b: string): number {
  if (!a || !b) return 0;
  const m = a.length;
  const n = b.length;
  const dp: number[][] = Array.from({ length: m + 1 }, () => new Array(n + 1).fill(0));
  for (let i = 0; i <= m; i++) dp[i][0] = i;
  for (let j = 0; j <= n; j++) dp[0][j] = j;
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      dp[i][j] =
        a[i - 1] === b[j - 1]
          ? dp[i - 1][j - 1]
          : 1 + Math.min(dp[i - 1][j], dp[i][j - 1], dp[i - 1][j - 1]);
    }
  }
  return 1 - dp[m][n] / Math.max(m, n);
}

/**
 * Best-effort reconstruction of cluster membership at an earlier round.
 *
 * Takes the terminal synthesis clusters as the ground-truth labels, then
 * walks each pseudonym's transcript responses up to `round`, matches their
 * last stated position against each cluster's text by simple substring +
 * Levenshtein ratio, and rebuilds per-node support counts accordingly.
 *
 * This is explicitly an inference. The real authoritative source is a
 * per-round synthesis pass (deferred to a follow-up PR).
 */
export function reconstructGraphAtRound(
  synthesis: SynthesisData,
  transcript: TranscriptResponse,
  round: number,
): GraphState {
  const base = deriveGraph(synthesis, transcript);
  const totalRounds = transcript.rounds?.length ?? 0;
  if (totalRounds === 0 || round < 0 || round >= totalRounds - 1) return base;

  // Build each pseudonym's last-known position up to and including `round`.
  const lastPos: Record<string, string> = {};
  for (const r of transcript.rounds) {
    if (r.round_number > round) break;
    for (const resp of r.responses) {
      if (resp.abstained) continue;
      const pc = resp.position_change;
      const summary =
        pc?.to_summary?.trim() || resp.response.slice(0, 400);
      if (summary) lastPos[resp.pseudonym] = summary;
    }
  }

  const matchesCluster = (pseudo: string, clusterText: string, threshold: number): boolean => {
    const pos = lastPos[pseudo];
    if (!pos) return false;
    const a = pos.toLowerCase();
    const b = clusterText.toLowerCase();
    if (!b) return false;
    // A cheap short-circuit: substring match dominates the Levenshtein cost.
    if (a.includes(b.slice(0, Math.min(40, b.length)))) return true;
    return levRatio(a, b) >= threshold;
  };

  return {
    nodes: base.nodes.map((n) => {
      if (n.kind === 'topic') return n;

      const threshold = n.kind === 'contested' ? 0.15 : 0.2;
      const supporters = n.supporters.filter((pseudo) =>
        matchesCluster(pseudo, n.fullText, threshold),
      );
      return { ...n, supporters, support: supporters.length };
    }),
    edges: base.edges,
  };
}
