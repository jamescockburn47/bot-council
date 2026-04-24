<script lang="ts">
  import type {
    DebateResponse,
    SynthesisResponse,
    TranscriptResponse,
  } from '$lib/types';
  import type { GraphNode, GraphState, NodeKind } from '$lib/argument-graph/types';
  import { deriveGraph } from '$lib/argument-graph/derive';
  import { reconstructGraphAtRound } from '$lib/argument-graph/reconstruct';
  import ArgumentMap3D from './ArgumentMap3D.svelte';
  import BotStanceMap from './BotStanceMap.svelte';
  import DivergenceHeadline from './DivergenceHeadline.svelte';
  import MapLegend from './MapLegend.svelte';
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

  // Outcome-tab sub-view: Arguments (3D graph of claims) or Positions
  // (per-bot confidence / reversal matrix). Two graphs, one click apart.
  let outcomeView = $state<'arguments' | 'positions'>('arguments');

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
  <div class="card-term" style="padding: 40px; text-align: center;">
    <h3 style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-txt); margin-bottom: 8px;">
      Outcome map will render once the debate completes
    </h3>
    <p class="mono-label" style="color: var(--glow-mute);">
      Status: {debate.status}. Switch to the Transcript tab to follow the
      debate in progress.
    </p>
  </div>
{:else if !synthesis || !graph}
  <div class="card-term" style="padding: 40px; text-align: center;">
    <h3 style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-txt); margin-bottom: 8px;">
      Synthesis not available
    </h3>
    <p class="mono-label" style="color: var(--glow-mute);">
      The synthesis engine did not produce a result for this debate. Check
      the Transcript tab for the raw rounds.
    </p>
  </div>
{:else}
  {#if synthesis.synthesis.executive_summary && synthesis.synthesis.executive_summary.trim()}
    <section
      class="card-term-lg"
      style="margin-bottom: 24px;"
      aria-label="Debate outcome summary"
    >
      <h3 class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 8px;">
        Outcome
      </h3>
      <p
        style="font-family: var(--serif-editorial); font-size: 17px; line-height: 1.55; color: var(--glow-txt); white-space: pre-wrap;"
      >
        {synthesis.synthesis.executive_summary}
      </p>
    </section>
  {/if}

  <DivergenceHeadline synthesis={synthesis.synthesis} />

  <!-- Sub-tabs: Arguments (3D map) / Positions (reversal matrix). -->
  <div class="flex gap-2 mb-4">
    <button
      onclick={() => (outcomeView = 'arguments')}
      class={outcomeView === 'arguments' ? 'pill-on' : 'pill-off'}
    >
      Arguments
    </button>
    <button
      onclick={() => (outcomeView = 'positions')}
      class={outcomeView === 'positions' ? 'pill-on' : 'pill-off'}
      disabled={!transcript || transcript.rounds.length === 0}
    >
      Positions
    </button>
  </div>

  {#if outcomeView === 'arguments'}
    <OutcomeFilters
      {hiddenKinds}
      {supporters}
      {highlightedSupporter}
      onToggleKind={toggleKind}
      onSupporterChange={(p) => (highlightedSupporter = p)}
    />

    <MapLegend />

    <ArgumentMap3D
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
  {:else if transcript}
    <BotStanceMap {transcript} {synthesis} />
  {/if}

  <OutcomeDrawer
    node={selectedNode}
    disagreement={selectedDisagreement}
    onClose={closeDrawer}
  />
{/if}
