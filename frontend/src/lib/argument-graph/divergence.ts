import type { SynthesisData } from '$lib/types';

/** Headline metrics for the Outcome tab's divergence panel.
 *
 * The framing is deliberate: LQ Council's USP is surfacing the interesting,
 * unsettled bits rather than averaging them away into a "median-slop"
 * answer. So we report signals of divergence (reversals, disagreements,
 * preserved minority voices) with equal weight to signals of settlement
 * (consensus points), not as an asterisk on them.
 */
export interface DivergenceSignals {
  consensus: number;
  disagreements: number;
  reversals: number;
  unjustifiedReversals: number;
  minorityVoices: number;
  /** 0..100 — a single scalar summary for a glance. */
  divergenceScore: number;
}

export function computeDivergence(s: SynthesisData | null | undefined): DivergenceSignals {
  const consensus = s?.consensus_points?.length ?? 0;
  const disagreements = s?.live_disagreements?.length ?? 0;
  const caps = s?.flagged_capitulations ?? [];
  const reversals = caps.length;
  const unjustifiedReversals = caps.filter((c) => !c.justification_adequate).length;
  const minorityVoices = s?.minority_positions?.length ?? 0;

  // Weighted blend. Disagreements count double because they are unresolved
  // at the end; minority voices count for the same reason (dissent
  // preserved). Unjustified reversals are flagged extra hard — they're
  // the "sycophantic collapse" anti-pattern the protocol is designed to
  // detect. Consensus is the denominator, not a subtraction, so a debate
  // with no consensus and lots of disagreement reads as HIGH divergence.
  const divergent = disagreements * 2 + minorityVoices * 2 + unjustifiedReversals * 3 + reversals;
  const total = divergent + consensus;
  const divergenceScore = total === 0 ? 0 : Math.round((divergent / total) * 100);

  return {
    consensus,
    disagreements,
    reversals,
    unjustifiedReversals,
    minorityVoices,
    divergenceScore,
  };
}
