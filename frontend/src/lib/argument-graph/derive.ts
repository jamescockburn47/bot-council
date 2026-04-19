import type { SynthesisData, TranscriptResponse } from '$lib/types';
import type { GraphEdge, GraphNode, GraphState } from './types';
import { truncate } from './types';

/**
 * Build the canonical terminal-state graph from a synthesis result.
 *
 * Nodes are argument-first: topic at centre, one node per consensus point,
 * two nodes per live disagreement (side_a / side_b), and one per minority
 * position. Pseudonyms appear only in node metadata, never as nodes.
 */
export function deriveGraph(
  synthesis: SynthesisData,
  transcript: TranscriptResponse | null,
): GraphState {
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];

  const totalBots = transcript?.anonymisation_log.length ?? 0;
  const allPseudonyms = transcript?.anonymisation_log.map((e) => e.pseudonym) ?? [];

  nodes.push({
    id: 'topic',
    kind: 'topic',
    label: 'Topic',
    fullText: synthesis.topic ?? '',
    support: totalBots,
    totalBots,
    confidence: null,
    supporters: allPseudonyms,
    bestArgument: null,
    evidence: null,
  });

  (synthesis.consensus_points ?? []).forEach((cp, i) => {
    const id = `consensus:${i}`;
    nodes.push({
      id,
      kind: 'consensus',
      label: truncate(cp.point ?? '', 40),
      fullText: cp.point ?? '',
      support: cp.supporting_bots?.length ?? 0,
      totalBots,
      confidence: null,
      supporters: cp.supporting_bots ?? [],
      bestArgument: null,
      evidence: cp.evidence ?? null,
    });
    edges.push({
      id: `e:topic-${id}`,
      source: 'topic',
      target: id,
      kind: 'topic-consensus',
      dashed: false,
    });
  });

  // Pairwise consensus soft-links (cap to avoid visual clutter on dense debates)
  const consensusIds = nodes.filter((n) => n.kind === 'consensus').map((n) => n.id);
  for (let i = 0; i < consensusIds.length - 1 && i < 4; i++) {
    edges.push({
      id: `e:clink-${i}`,
      source: consensusIds[i],
      target: consensusIds[i + 1],
      kind: 'consensus-link',
      dashed: false,
    });
  }

  (synthesis.live_disagreements ?? []).forEach((d, i) => {
    const aId = `side_a:${i}`;
    const bId = `side_b:${i}`;

    nodes.push({
      id: aId,
      kind: 'contested',
      label: truncate(d.side_a?.position ?? '', 40),
      fullText: d.side_a?.position ?? '',
      support: d.side_a?.bots?.length ?? 0,
      totalBots,
      confidence: null,
      supporters: d.side_a?.bots ?? [],
      bestArgument: d.side_a?.best_argument ?? null,
      evidence: null,
      disagreementIssue: d.issue,
      sideKey: 'a',
      pairIndex: i,
    });
    nodes.push({
      id: bId,
      kind: 'contested',
      label: truncate(d.side_b?.position ?? '', 40),
      fullText: d.side_b?.position ?? '',
      support: d.side_b?.bots?.length ?? 0,
      totalBots,
      confidence: null,
      supporters: d.side_b?.bots ?? [],
      bestArgument: d.side_b?.best_argument ?? null,
      evidence: null,
      disagreementIssue: d.issue,
      sideKey: 'b',
      pairIndex: i,
    });

    edges.push({
      id: `e:topic-${aId}`,
      source: 'topic',
      target: aId,
      kind: 'topic-contested',
      dashed: true,
    });
    edges.push({
      id: `e:topic-${bId}`,
      source: 'topic',
      target: bId,
      kind: 'topic-contested',
      dashed: true,
    });
    edges.push({
      id: `e:tension-${i}`,
      source: aId,
      target: bId,
      kind: 'tension',
      dashed: true,
    });
  });

  (synthesis.minority_positions ?? []).forEach((m, i) => {
    const id = `minority:${i}`;
    nodes.push({
      id,
      kind: 'minority',
      label: truncate(m.position ?? '', 40),
      fullText: m.position ?? '',
      support: 1,
      totalBots,
      confidence: m.confidence ?? null,
      supporters: m.bot ? [m.bot] : [],
      bestArgument: m.key_argument ?? null,
      evidence: null,
    });
    edges.push({
      id: `e:topic-${id}`,
      source: 'topic',
      target: id,
      kind: 'topic-minority',
      dashed: true,
    });
  });

  return { nodes, edges };
}
