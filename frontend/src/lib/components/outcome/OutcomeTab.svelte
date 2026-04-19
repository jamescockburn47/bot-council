<script lang="ts">
  import type {
    DebateResponse,
    SynthesisResponse,
    TranscriptResponse,
  } from '$lib/types';
  import type { GraphNode, GraphState, NodeKind } from '$lib/argument-graph/types';
  import { deriveGraph } from '$lib/argument-graph/derive';
  import { reconstructGraphAtRound } from '$lib/argument-graph/reconstruct';
  import ArgumentMap from './ArgumentMap.svelte';
  import ReplaySlider from './ReplaySlider.svelte';
  import OutcomeFilters from './OutcomeFilters.svelte';
  import OutcomeDrawer from './OutcomeDrawer.svelte';

  let {
    debate,
    synthesis,
    transcript,
  }: {
    debate: DebateResponse;
    synthesis: SynthesisResponse | null;
    transcript: TranscriptResponse | null;
  } = $props();

  const TERMINAL = ['complete', 'cancelled', 'failed'];
  let isTerminal = $derived(TERMINAL.includes(debate.status));

  // Round state: -1 = Final (terminal synthesis), 0..N-1 = reconstruction
  let selectedRound = $state(-1);
  let playing = $state(false);

  // Filters
  let hiddenKinds = $state<Set<NodeKind>>(new Set());
  let highlightedSupporter = $state<string | null>(null);

  // Selection
  let selectedNodeId = $state<string | null>(null);
  let selectedEdgeId = $state<string | null>(null);

  let totalRounds = $derived(transcript?.rounds?.length ?? 0);

  let graph: GraphState | null = $derived.by(() => {
    if (!synthesis || !synthesis.synthesis) return null;
    if (selectedRound === -1 || !transcript) {
      return deriveGraph(synthesis.synthesis, transcript);
    }
    return reconstructGraphAtRound(
      synthesis.synthesis,
      transcript,
      selectedRound,
    );
  });

  let supporters = $derived<string[]>(
    transcript?.anonymisation_log.map((e) => e.pseudonym) ?? [],
  );

  let selectedNode: GraphNode | null = $derived.by(() => {
    if (!graph || !selectedNodeId) return null;
    return graph.nodes.find((n) => n.id === selectedNodeId) ?? null;
  });

  let selectedDisagreement = $derived.by(() => {
    if (!graph || !selectedEdgeId) return null;
    const edge = graph.edges.find((e) => e.id === selectedEdgeId);
    if (!edge || edge.kind !== 'tension') return null;
    const sourceId = typeof edge.source === 'string' ? edge.source : edge.source.id;
    const targetId = typeof edge.target === 'string' ? edge.target : edge.target.id;
    const sideA = graph.nodes.find((n) => n.id === sourceId);
    const sideB = graph.nodes.find((n) => n.id === targetId);
    if (!sideA || !sideB) return null;
    return {
      issue: sideA.disagreementIssue ?? '',
      sideA,
      sideB,
    };
  });

  function handleNodeClick(id: string) {
    selectedEdgeId = null;
    selectedNodeId = selectedNodeId === id ? null : id;
  }

  function handleEdgeClick(id: string) {
    selectedNodeId = null;
    selectedEdgeId = selectedEdgeId === id ? null : id;
  }

  function closeDrawer() {
    selectedNodeId = null;
    selectedEdgeId = null;
  }

  function toggleKind(kind: NodeKind) {
    const next = new Set(hiddenKinds);
    if (next.has(kind)) next.delete(kind);
    else next.add(kind);
    hiddenKinds = next;
  }
</script>

{#if !isTerminal}
  <div
    class="bg-[var(--surface)] border border-[var(--border)] rounded-xl p-10 text-center"
  >
    <h3 class="text-sm text-[var(--text-primary)] mb-2">
      Outcome map will render once the debate completes
    </h3>
    <p class="text-xs mono text-[var(--text-muted)]">
      Status: {debate.status}. Switch to the Transcript tab to follow the
      debate in progress.
    </p>
  </div>
{:else if !synthesis || !graph}
  <div
    class="bg-[var(--surface)] border border-[var(--border)] rounded-xl p-10 text-center"
  >
    <h3 class="text-sm text-[var(--text-primary)] mb-2">
      Synthesis not available
    </h3>
    <p class="text-xs mono text-[var(--text-muted)]">
      The synthesis engine did not produce a result for this debate. Check
      the Transcript tab for the raw rounds.
    </p>
  </div>
{:else}
  <OutcomeFilters
    {hiddenKinds}
    {supporters}
    {highlightedSupporter}
    onToggleKind={toggleKind}
    onSupporterChange={(p) => (highlightedSupporter = p)}
  />

  <ArgumentMap
    {graph}
    {selectedNodeId}
    {hiddenKinds}
    {highlightedSupporter}
    onNodeClick={handleNodeClick}
    onEdgeClick={handleEdgeClick}
  />

  {#if totalRounds > 1}
    <ReplaySlider
      rounds={totalRounds}
      round={selectedRound}
      {playing}
      inferred={selectedRound !== -1}
      onRoundChange={(r) => (selectedRound = r)}
      onPlayToggle={() => (playing = !playing)}
    />
  {/if}

  <OutcomeDrawer
    node={selectedNode}
    disagreement={selectedDisagreement}
    onClose={closeDrawer}
  />
{/if}
