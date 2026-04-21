// d3-force-3d has no published type declarations. We only use a tiny
// slice of its surface — the directional-attraction forceX / forceY /
// forceZ constructors — and 3d-force-graph accepts them as untyped
// objects anyway. Declaring the module with a permissive shape keeps
// svelte-check happy without pulling a wholesale .d.ts.
declare module 'd3-force-3d' {
  export interface Force {
    strength(arg: number | ((n: unknown, i: number) => number)): Force;
  }
  export function forceX(x?: number): Force;
  export function forceY(y?: number): Force;
  export function forceZ(z?: number): Force;
  export function forceManyBody(): Force;
  export function forceCollide(r?: number): Force;
  export function forceLink<N, E>(edges?: E[]): Force;
  export function forceCenter(x?: number, y?: number, z?: number): Force;
}
