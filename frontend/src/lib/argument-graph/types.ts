export type NodeKind = 'topic' | 'consensus' | 'contested' | 'minority';

export interface GraphNode {
  id: string;
  kind: NodeKind;
  label: string;
  fullText: string;
  support: number;
  totalBots: number;
  confidence: number | null;
  supporters: string[];
  bestArgument: string | null;
  evidence: string | null;
  disagreementIssue?: string;
  sideKey?: 'a' | 'b';
  pairIndex?: number;

  // Simulation-populated positions. Present after the first tick.
  x?: number;
  y?: number;
  vx?: number;
  vy?: number;
  fx?: number | null;
  fy?: number | null;
}

export type EdgeKind =
  | 'topic-consensus'
  | 'topic-contested'
  | 'topic-minority'
  | 'consensus-link'
  | 'tension';

export interface GraphEdge {
  id: string;
  source: string | GraphNode;
  target: string | GraphNode;
  kind: EdgeKind;
  dashed: boolean;
}

export interface GraphState {
  nodes: GraphNode[];
  edges: GraphEdge[];
}

export function truncate(s: string, n: number): string {
  return s.length <= n ? s : s.slice(0, n - 1) + '…';
}

export function colourFor(kind: NodeKind): string {
  switch (kind) {
    case 'topic':
      return '#e4e4e7';
    case 'consensus':
      return '#10b981';
    case 'contested':
      return '#f43f5e';
    case 'minority':
      return '#6366F1';
  }
}
