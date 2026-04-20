<script lang="ts">
  import SynthesisCard from '$lib/components/SynthesisCard.svelte';
  import ConfidenceChart from '$lib/components/ConfidenceChart.svelte';
  import RoundAccordion from '$lib/components/RoundAccordion.svelte';
  import DivergencePanel from '$lib/components/DivergencePanel.svelte';
  import RawJsonToggle from '$lib/components/RawJsonToggle.svelte';
  import type {
    DebateResponse,
    TranscriptResponse,
    SynthesisResponse,
  } from '$lib/types';

  let {
    debate,
    transcript,
    synthesis,
  }: {
    debate: DebateResponse;
    transcript: TranscriptResponse | null;
    synthesis: SynthesisResponse | null;
  } = $props();

  const TERMINAL = ['complete', 'cancelled', 'failed'];
  let isTerminal = $derived(TERMINAL.includes(debate.status));

  let anonLogExpanded = $state(false);

  let roleMap = $derived.by<Record<string, string | null>>(() => {
    if (!transcript) return {};
    const map: Record<string, string | null> = {};
    for (const entry of transcript.anonymisation_log) {
      map[entry.pseudonym] = entry.role;
    }
    return map;
  });

  let consensusItems = $derived(
    synthesis?.synthesis?.consensus_points?.map((p) => ({
      label: p.point ?? '',
      detail: `${(p.supporting_bots ?? []).join(', ')} -- ${p.evidence ?? ''}`,
    })) ?? [],
  );

  let disagreementItems = $derived(
    synthesis?.synthesis?.live_disagreements?.map((d) => ({
      label: d.issue ?? '',
      detail: `${(d.side_a?.bots ?? []).join(', ')}: "${d.side_a?.position ?? ''}" vs ${(d.side_b?.bots ?? []).join(', ')}: "${d.side_b?.position ?? ''}"`,
    })) ?? [],
  );

  let capitulationItems = $derived(
    synthesis?.synthesis?.flagged_capitulations?.map((c) => ({
      label: `${c.bot ?? '?'}: ${c.from ?? '?'} -> ${c.to ?? '?'}`,
      detail: `${c.justification_adequate ? 'Justified' : 'Unjustified'} -- ${c.flag_reason ?? ''}`,
    })) ?? [],
  );

  let minorityItems = $derived(
    synthesis?.synthesis?.minority_positions?.map((m) => ({
      label: `${m.bot ?? '?'} (conf: ${m.confidence ?? '?'})`,
      detail: `${m.position ?? ''} -- ${m.key_argument ?? ''}`,
    })) ?? [],
  );
</script>

<!-- Synthesis section -->
{#if synthesis}
  <div class="grid grid-cols-2 gap-4 mb-6">
    <SynthesisCard
      title="Consensus"
      count={synthesis.synthesis?.consensus_points?.length ?? 0}
      color="#22c55e"
      items={consensusItems}
    />
    <SynthesisCard
      title="Disagreements"
      count={synthesis.synthesis?.live_disagreements?.length ?? 0}
      color="#ef4444"
      items={disagreementItems}
    />
    <SynthesisCard
      title="Capitulations"
      count={synthesis.synthesis?.flagged_capitulations?.length ?? 0}
      color="#f59e0b"
      items={capitulationItems}
    />
    <SynthesisCard
      title="Minority Positions"
      count={synthesis.synthesis?.minority_positions?.length ?? 0}
      color="#8b5cf6"
      items={minorityItems}
    />
  </div>

  <!-- Confidence trajectories -->
  {#if synthesis.synthesis?.confidence_trajectories && Object.keys(synthesis.synthesis.confidence_trajectories).length > 0}
    <div class="mb-6">
      <ConfidenceChart trajectories={synthesis.synthesis.confidence_trajectories} />
    </div>
  {/if}

  <!-- Meta observations -->
  {#if synthesis.synthesis?.meta_observations}
    <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-4 mb-6">
      <h3 class="text-xs mono text-[var(--text-muted)] mb-2 uppercase tracking-wider">
        Meta Observations
      </h3>
      <p
        class="text-sm text-[var(--text-secondary)] whitespace-pre-wrap leading-relaxed"
      >
        {synthesis.synthesis.meta_observations}
      </p>
    </div>
  {/if}

  <!-- Citation check -->
  {#if synthesis.citation_check}
    {@const cc = synthesis.citation_check}
    {@const allValid = cc.citations_invalid.length === 0}
    <div class="mb-6 flex items-center gap-2">
      {#if allValid}
        <span
          class="text-xs mono px-2 py-1 rounded bg-green-500/10 text-green-400 border border-green-500/20"
        >
          All {cc.citations_total} citations valid
        </span>
      {:else}
        <span
          class="text-xs mono px-2 py-1 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20"
        >
          {cc.citations_invalid.length} invalid citation{cc.citations_invalid.length !== 1 ? 's' : ''} of {cc.citations_total}
        </span>
      {/if}
    </div>
  {/if}

  <RawJsonToggle data={synthesis} />
  <div class="mb-8"></div>
{:else if !isTerminal}
  <div
    class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-6 mb-8 text-center"
  >
    <p class="text-sm text-[var(--text-muted)] mono">
      Synthesis will appear when all rounds complete.
    </p>
  </div>
{/if}

<!-- Transcript -->
{#if transcript}
  <h2
    class="text-xs mono text-[var(--text-muted)] uppercase tracking-wider mb-4"
  >
    Transcript
  </h2>
  <div class="space-y-3 mb-8">
    {#each transcript.rounds as round (round.round_number)}
      <RoundAccordion {round} {roleMap} />
    {/each}
  </div>

  <!-- Divergence analysis -->
  {#if transcript.divergence_analyses.length > 0}
    <div class="mb-8">
      <DivergencePanel analyses={transcript.divergence_analyses} {roleMap} />
    </div>
  {/if}

  <!-- Anonymisation log -->
  <div class="border border-[var(--border)] rounded-lg overflow-hidden mb-8">
    <button
      onclick={() => (anonLogExpanded = !anonLogExpanded)}
      class="w-full flex items-center justify-between px-4 py-3 bg-[var(--surface)] hover:bg-[var(--surface-hover)] transition-colors text-left"
    >
      <span class="text-sm font-medium text-[var(--text-primary)]">Anonymisation Log</span>
      <span class="text-xs mono text-[var(--text-muted)]">{anonLogExpanded ? '-' : '+'}</span>
    </button>
    {#if anonLogExpanded}
      <div class="p-4">
        <table class="w-full text-xs mono">
          <thead>
            <tr class="text-[var(--text-muted)]">
              <th class="text-left pb-2">Pseudonym</th>
              <th class="text-left pb-2">Role</th>
            </tr>
          </thead>
          <tbody>
            {#each transcript.anonymisation_log as entry (entry.pseudonym)}
              <tr class="border-t border-[var(--border)]">
                <td class="py-1.5 text-[var(--text-secondary)]">{entry.pseudonym}</td>
                <td class="py-1.5 text-[var(--text-muted)]">{entry.role ?? 'none'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
{/if}
