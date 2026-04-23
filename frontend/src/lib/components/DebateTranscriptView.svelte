<script lang="ts">
  import SynthesisCard from '$lib/components/SynthesisCard.svelte';
  import ConfidenceChart from '$lib/components/ConfidenceChart.svelte';
  import RoundAccordion from '$lib/components/RoundAccordion.svelte';
  import DivergencePanel from '$lib/components/DivergencePanel.svelte';
  import RawJsonToggle from '$lib/components/RawJsonToggle.svelte';
  import SynthesisQualityReport from '$lib/components/SynthesisQualityReport.svelte';
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
      color="var(--indigo-400)"
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
    <div class="card-term mb-6">
      <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 8px;">Meta Observations</p>
      <p
        style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-dim); white-space: pre-wrap; line-height: 1.65;"
      >
        {synthesis.synthesis.meta_observations}
      </p>
    </div>
  {/if}

  <!-- Synthesis quality: citation verification, extraction outcomes, reliability -->
  <SynthesisQualityReport
    citationCheck={synthesis.citation_check}
    transcript={transcript}
  />

  <RawJsonToggle data={synthesis} />
  <div class="mb-8"></div>
{:else if !isTerminal}
  <div class="card-term mb-8 text-center">
    <p class="mono-label">
      Synthesis will appear when all rounds complete.
    </p>
  </div>
{/if}

<!-- Transcript -->
{#if transcript}
  <p class="tm-eyebrow mb-4" style="color: var(--indigo-400);">Transcript</p>
  <div class="space-y-3 mb-8">
    {#each transcript.rounds as round (round.round_number)}
      <RoundAccordion {round} {roleMap} crux={transcript.crux ?? null} />
    {/each}
  </div>

  <!-- Divergence analysis -->
  {#if transcript.divergence_analyses.length > 0}
    <div class="mb-8">
      <DivergencePanel analyses={transcript.divergence_analyses} {roleMap} />
    </div>
  {/if}

  <!-- Anonymisation log -->
  <div class="card-term mb-8" style="padding: 0; overflow: hidden;">
    <button
      onclick={() => (anonLogExpanded = !anonLogExpanded)}
      class="w-full flex items-center justify-between px-4 py-3 text-left transition-colors"
      style="background: var(--night-raise);"
      onmouseenter={(e) => (e.currentTarget.style.background = 'var(--night-edge)')}
      onmouseleave={(e) => (e.currentTarget.style.background = 'var(--night-raise)')}
    >
      <span style="font-family: var(--sans-product); font-size: 14px; font-weight: 500; color: var(--glow-txt);">Anonymisation Log</span>
      <span class="mono-label">{anonLogExpanded ? '-' : '+'}</span>
    </button>
    {#if anonLogExpanded}
      <div class="p-4" style="border-top: 1px solid var(--night-rule);">
        <table class="w-full text-xs mono">
          <thead>
            <tr>
              <th class="text-left pb-2 mono-label">Pseudonym</th>
              <th class="text-left pb-2 mono-label">Role</th>
            </tr>
          </thead>
          <tbody>
            {#each transcript.anonymisation_log as entry (entry.pseudonym)}
              <tr style="border-top: 1px solid var(--night-rule);">
                <td class="py-1.5" style="color: var(--glow-dim);">{entry.pseudonym}</td>
                <td class="py-1.5" style="color: var(--glow-faint);">{entry.role ?? 'none'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </div>
{/if}
