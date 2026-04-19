import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type Simulation,
} from 'd3-force';
import type { GraphEdge, GraphNode } from './types';

export interface SimulationHandle {
  sim: Simulation<GraphNode, GraphEdge>;
  stop(): void;
}

/**
 * Set up a d3-force simulation with custom attractor geometry that nudges
 * consensus nodes into a cluster, pulls disagreement pairs apart along a
 * tension axis, and lets minority nodes drift to the periphery.
 *
 * The caller mutates `nodes` in place — d3-force assigns x/y/vx/vy — and
 * is expected to re-render reactively on each tick via `onTick`.
 */
export function createSimulation(
  nodes: GraphNode[],
  edges: GraphEdge[],
  width: number,
  height: number,
  onTick: () => void,
): SimulationHandle {
  const cx = width / 2;
  const cy = height / 2;

  // Anchor the topic at the centre.
  const topic = nodes.find((n) => n.kind === 'topic');
  if (topic) {
    topic.fx = cx;
    topic.fy = cy;
  }

  const sim = forceSimulation<GraphNode>(nodes)
    .force('charge', forceManyBody().strength(-220))
    .force(
      'collide',
      forceCollide<GraphNode>().radius((n) => nodeRadius(n) + 18),
    )
    .force(
      'link',
      forceLink<GraphNode, GraphEdge>(edges)
        .id((d) => d.id)
        .distance((e) => linkDistance(e))
        .strength(0.35),
    )
    // Consensus cluster: pull right of centre, up from centre.
    .force(
      'attract-consensus-x',
      forceX<GraphNode>(cx + 160).strength((n) => (n.kind === 'consensus' ? 0.055 : 0)),
    )
    .force(
      'attract-consensus-y',
      forceY<GraphNode>(cy - 70).strength((n) => (n.kind === 'consensus' ? 0.055 : 0)),
    )
    // Minority drifts far left.
    .force(
      'attract-minority-x',
      forceX<GraphNode>(cx - 280).strength((n) => (n.kind === 'minority' ? 0.06 : 0)),
    )
    .force(
      'attract-minority-y',
      forceY<GraphNode>(cy + 90).strength((n) => (n.kind === 'minority' ? 0.04 : 0)),
    )
    // Disagreement pairs: side A left-of-centre, side B right-of-centre.
    .force(
      'attract-contested-a',
      forceX<GraphNode>(cx - 200).strength((n) =>
        n.kind === 'contested' && n.sideKey === 'a' ? 0.07 : 0,
      ),
    )
    .force(
      'attract-contested-b',
      forceX<GraphNode>(cx + 240).strength((n) =>
        n.kind === 'contested' && n.sideKey === 'b' ? 0.07 : 0,
      ),
    )
    .force(
      'attract-contested-y',
      forceY<GraphNode>(cy + 60).strength((n) => (n.kind === 'contested' ? 0.03 : 0)),
    )
    .alphaMin(0.01)
    .alphaDecay(0.04);

  sim.on('tick', onTick);
  return {
    sim,
    stop: () => sim.stop(),
  };
}

/**
 * Visual radius for a node. Topic is fixed; others scale with support count.
 */
export function nodeRadius(n: GraphNode): number {
  if (n.kind === 'topic') return 28;
  const base = 10;
  const boost = Math.min(n.support * 2.5, 14);
  return base + boost;
}

function linkDistance(e: GraphEdge): number {
  switch (e.kind) {
    case 'topic-consensus':
      return 130;
    case 'topic-contested':
      return 190;
    case 'topic-minority':
      return 240;
    case 'consensus-link':
      return 90;
    case 'tension':
      return 260;
  }
}
