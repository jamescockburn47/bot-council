<script lang="ts">
  import { api, debateStreamUrl } from '$lib/api/client';
  import StatusBadge from '$lib/components/StatusBadge.svelte';
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

  let { data } = $props();

  let debate = $state<DebateResponse | null>(null);
  let transcript = $state<TranscriptResponse | null>(null);
  let synthesis = $state<SynthesisResponse | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let anonLogExpanded = $state(false);
  let sseConnected = $state(false);

  const TERMINAL = ['complete', 'cancelled', 'failed'];

  function formatDate(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' }) +
      ' ' + d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
  }

  let roleMap = $derived.by<Record<string, string | null>>(() => {
    if (!transcript) return {};
    const map: Record<string, string | null> = {};
    for (const entry of transcript.anonymisation_log) {
      map[entry.pseudonym] = entry.role;
    }
    return map;
  });

  let isTerminal = $derived(debate ? TERMINAL.includes(debate.status) : false);

  function isTerminalStatus(status: string): boolean {
    return TERMINAL.includes(status);
  }

  // Initial fetch
  $effect(() => {
    const id = data.debateId;
    loadAll(id);
  });

  async function loadAll(id: string) {
    try {
      const [d, t] = await Promise.all([
        api.debates.get(id),
        api.debates.transcript(id),
      ]);
      debate = d;
      transcript = t;

      if (TERMINAL.includes(d.status)) {
        try {
          synthesis = await api.debates.synthesis(id);
        } catch {
          // synthesis may not exist yet -- that's fine
        }
      }
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : 'Failed to load debate';
    } finally {
      loading = false;
    }
  }

  // EventSource for live debate updates
  $effect(() => {
    if (!debate || isTerminalStatus(debate.status)) return;

    const es = new EventSource(debateStreamUrl(data.debateId));

    es.onopen = () => { sseConnected = true; };
    es.onerror = () => { sseConnected = false; };

    es.addEventListener('round:started', (e: MessageEvent) => {
      const d = JSON.parse(e.data);
      if (transcript) {
        const exists = transcript.rounds.find((r: any) => r.round_number === d.round_number);
        if (!exists) {
          transcript.rounds = [...transcript.rounds, {
            round_number: d.round_number,
            status: 'in_progress',
            responses: [],
          }];
        }
      }
    });

    es.addEventListener('response:received', (e: MessageEvent) => {
      const d = JSON.parse(e.data);
      if (transcript) {
        const round = transcript.rounds.find((r: any) => r.round_number === d.round_number);
        if (round) {
          round.responses = [...round.responses, {
            pseudonym: d.pseudonym,
            response: d.response,
            confidence: d.confidence ?? null,
            challenge: d.challenge ?? null,
            position_change: d.position_change ?? null,
            valid: d.valid,
            abstained: d.abstained,
            validation_reasoning: null,
          }];
          transcript = transcript; // trigger Svelte 5 reactivity
        }
      }
    });

    es.addEventListener('round:completed', (e: MessageEvent) => {
      const d = JSON.parse(e.data);
      if (transcript) {
        const round = transcript.rounds.find((r: any) => r.round_number === d.round_number);
        if (round) {
          round.status = 'complete';
          transcript = transcript;
        }
      }
    });

    es.addEventListener('synthesis:completed', (e: MessageEvent) => {
      const d = JSON.parse(e.data);
      synthesis = {
        debate_id: data.debateId,
        synthesis: d.synthesis,
        model_used: '',
        created_at: new Date().toISOString(),
        citation_check: d.citation_check ?? null,
      };
    });

    es.addEventListener('debate:completed', () => {
      if (debate) debate = { ...debate, status: 'complete' };
      sseConnected = false;
      es.close();
    });

    es.addEventListener('debate:failed', (e: MessageEvent) => {
      const d = JSON.parse(e.data);
      if (debate) debate = { ...debate, status: 'failed' };
      error = `Debate failed: ${d.reason}`;
      sseConnected = false;
      es.close();
    });

    return () => {
      es.close();
      sseConnected = false;
    };
  });

  // Synthesis card data
  let consensusItems = $derived(
    synthesis?.synthesis.consensus_points.map(p => ({
      label: p.point,
      detail: `${p.supporting_bots.join(', ')} -- ${p.evidence}`,
    })) ?? []
  );

  let disagreementItems = $derived(
    synthesis?.synthesis.live_disagreements.map(d => ({
      label: d.issue,
      detail: `${d.side_a.bots.join(', ')}: "${d.side_a.position}" vs ${d.side_b.bots.join(', ')}: "${d.side_b.position}"`,
    })) ?? []
  );

  let capitulationItems = $derived(
    synthesis?.synthesis.flagged_capitulations.map(c => ({
      label: `${c.bot}: ${c.from} -> ${c.to}`,
      detail: `${c.justification_adequate ? 'Justified' : 'Unjustified'} -- ${c.flag_reason}`,
    })) ?? []
  );

  let minorityItems = $derived(
    synthesis?.synthesis.minority_positions.map(m => ({
      label: `${m.bot} (conf: ${m.confidence})`,
      detail: `${m.position} -- ${m.key_argument}`,
    })) ?? []
  );

  const ROUND_LABELS: Record<number, string> = {
    0: 'Blind Formation',
    1: 'Anonymous Distribution',
    2: 'Structured Rebuttal',
    3: 'Cross-Examination',
    4: 'Final Position',
  };

  function buildMarkdownExport(): string {
    if (!debate || !transcript) return '';
    const lines: string[] = [];

    lines.push(`# ${debate.topic}`);
    lines.push('');
    lines.push(`**Status:** ${debate.status}  `);
    lines.push(`**Debate ID:** ${debate.id}  `);
    lines.push(`**Created:** ${formatDate(debate.created_at)}  `);
    if (debate.completed_at) lines.push(`**Completed:** ${formatDate(debate.completed_at)}  `);
    lines.push('');

    // Participants
    lines.push('## Participants');
    lines.push('');
    lines.push('| Agent | Role |');
    lines.push('|-------|------|');
    for (const entry of transcript.anonymisation_log) {
      const bot = debate.bots.find(b => b.pseudonym === entry.pseudonym);
      const name = bot?.bot_name ?? entry.pseudonym;
      lines.push(`| ${name} (${entry.pseudonym}) | ${entry.role ?? 'none'} |`);
    }
    lines.push('');

    // Synthesis
    if (synthesis) {
      const s = synthesis.synthesis;
      lines.push('## Synthesis');
      lines.push('');
      lines.push(`*Model: ${synthesis.model_used}*`);
      lines.push('');

      if (s.consensus_points.length > 0) {
        lines.push('### Consensus Points');
        lines.push('');
        for (const cp of s.consensus_points) {
          lines.push(`**${cp.point}**`);
          lines.push(`> Supporting: ${cp.supporting_bots.join(', ')}`);
          lines.push(`> ${cp.evidence}`);
          lines.push('');
        }
      }

      if (s.live_disagreements.length > 0) {
        lines.push('### Live Disagreements');
        lines.push('');
        for (const d of s.live_disagreements) {
          lines.push(`**${d.issue}**`);
          lines.push('');
          lines.push(`*${d.side_a.bots.join(', ')}:* ${d.side_a.position}`);
          lines.push(`> ${d.side_a.best_argument}`);
          lines.push('');
          lines.push(`*${d.side_b.bots.join(', ')}:* ${d.side_b.position}`);
          lines.push(`> ${d.side_b.best_argument}`);
          lines.push('');
        }
      }

      if (s.flagged_capitulations.length > 0) {
        lines.push('### Flagged Capitulations');
        lines.push('');
        for (const c of s.flagged_capitulations) {
          lines.push(`**${c.bot}** ${c.justification_adequate ? '(justified)' : '(unjustified)'}`);
          lines.push(`- From: ${c.from}`);
          lines.push(`- To: ${c.to}`);
          lines.push(`- Reason: ${c.flag_reason}`);
          lines.push('');
        }
      }

      if (s.minority_positions.length > 0) {
        lines.push('### Minority Positions');
        lines.push('');
        for (const m of s.minority_positions) {
          lines.push(`**${m.bot}** (confidence: ${m.confidence})`);
          lines.push(`${m.position}`);
          lines.push(`> ${m.key_argument}`);
          lines.push('');
        }
      }

      if (s.meta_observations) {
        lines.push('### Meta Observations');
        lines.push('');
        lines.push(s.meta_observations);
        lines.push('');
      }
    }

    // Transcript
    lines.push('---');
    lines.push('');
    lines.push('## Transcript');
    lines.push('');

    for (const round of transcript.rounds) {
      const label = ROUND_LABELS[round.round_number] ?? `Round ${round.round_number}`;
      lines.push(`### Round ${round.round_number}: ${label}`);
      lines.push('');

      for (const resp of round.responses) {
        const role = roleMap[resp.pseudonym];
        const roleTag = role ? ` (${role})` : '';
        const confTag = resp.confidence != null ? ` [confidence: ${resp.confidence}]` : '';
        lines.push(`**${resp.pseudonym}${roleTag}**${confTag}`);
        if (resp.abstained) {
          lines.push('*Abstained*');
        } else {
          lines.push('');
          lines.push(resp.response);
        }
        lines.push('');

        if (resp.challenge) {
          lines.push(`> **Challenge** (${resp.challenge.type}): ${resp.challenge.claim_targeted}`);
          lines.push(`> ${resp.challenge.counter_evidence}`);
          lines.push('');
        }

        if (resp.position_change) {
          const pc = resp.position_change;
          lines.push(`> **Position ${pc.changed ? 'changed' : 'maintained'}**`);
          if (pc.changed) {
            lines.push(`> From: ${pc.from_summary}`);
            lines.push(`> To: ${pc.to_summary}`);
          }
          lines.push(`> Reason: ${pc.reason}`);
          lines.push('');
        }
      }
    }

    // Divergence
    if (transcript.divergence_analyses.length > 0) {
      lines.push('---');
      lines.push('');
      lines.push('## Divergence Analysis');
      lines.push('');
      lines.push('| Agent | Shifted | Magnitude | Justified |');
      lines.push('|-------|---------|-----------|-----------|');
      for (const d of transcript.divergence_analyses) {
        lines.push(`| ${d.pseudonym} | ${d.shifted ? 'Yes' : 'No'} | ${d.magnitude ?? '-'} | ${d.justification_adequate ? 'Yes' : 'No'} |`);
      }
      lines.push('');
      for (const d of transcript.divergence_analyses) {
        if (d.what_changed) {
          lines.push(`**${d.pseudonym}:** ${d.what_changed}`);
          if (d.flags.length > 0) {
            for (const f of d.flags) lines.push(`- Flag: ${f}`);
          }
          lines.push('');
        }
      }
    }

    return lines.join('\n');
  }

  function exportMarkdown() {
    const md = buildMarkdownExport();
    const blob = new Blob([md], { type: 'text/markdown;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    const slug = debate!.topic.slice(0, 40).replace(/[^a-zA-Z0-9]+/g, '-').toLowerCase();
    a.download = `debate-${slug}-${debate!.id.slice(0, 8)}.md`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

{#if loading}
  <div class="max-w-5xl space-y-4">
    <div class="animate-pulse">
      <div class="h-6 bg-[var(--border)] rounded w-1/4 mb-4"></div>
      <div class="h-8 bg-[var(--border)] rounded w-3/4 mb-6"></div>
      <div class="grid grid-cols-2 gap-4">
        {#each Array(4) as _}
          <div class="h-32 bg-[var(--surface)] border border-[var(--border)] rounded-lg"></div>
        {/each}
      </div>
    </div>
  </div>

{:else if error}
  <div class="max-w-5xl">
    <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-6 text-center">
      <p class="text-red-400 mono text-sm">{error}</p>
      <a
        href="/debates"
        class="inline-block mt-3 px-4 py-1.5 text-xs mono text-[var(--text-secondary)] border border-[var(--border)] rounded hover:text-[var(--text-primary)] transition-colors no-underline"
      >
        Back to debates
      </a>
    </div>
  </div>

{:else if debate}
  <div class="max-w-5xl">
    <!-- Header -->
    <div class="mb-8">
      <div class="flex items-center gap-3 mb-2">
        <a href="/debates" class="text-xs mono text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors no-underline">
          Debates
        </a>
        <span class="text-[var(--text-muted)]">/</span>
        <span class="text-xs mono text-[var(--text-muted)]">{debate.id.slice(0, 8)}</span>
        <StatusBadge status={debate.status} />
        {#if sseConnected}
          <span class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs mono text-[#22c55e] bg-[#22c55e15] border border-[#22c55e30]">
            <span class="w-1.5 h-1.5 rounded-full bg-[#22c55e] animate-pulse"></span>
            LIVE
          </span>
        {/if}
      </div>
      <h1 class="text-xl font-bold text-[var(--text-primary)] mb-3">{debate.topic}</h1>
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-4 text-[10px] mono text-[var(--text-muted)]">
          <span>{debate.bots.length} agent{debate.bots.length !== 1 ? 's' : ''}</span>
          {#if transcript}
            <span>{transcript.rounds.length} round{transcript.rounds.length !== 1 ? 's' : ''}</span>
          {/if}
          <span>Created {formatDate(debate.created_at)}</span>
          {#if debate.completed_at}
            <span>Completed {formatDate(debate.completed_at)}</span>
          {/if}
        </div>
        {#if isTerminal && transcript}
          <button
            onclick={exportMarkdown}
            class="px-3 py-1.5 text-xs mono text-[var(--text-secondary)] border border-[var(--border)] rounded hover:text-[var(--text-primary)] hover:border-[var(--text-muted)] transition-colors"
          >
            Export .md
          </button>
        {/if}
      </div>
    </div>

    <!-- Synthesis section -->
    {#if synthesis}
      <div class="grid grid-cols-2 gap-4 mb-6">
        <SynthesisCard
          title="Consensus"
          count={synthesis.synthesis.consensus_points.length}
          color="#22c55e"
          items={consensusItems}
        />
        <SynthesisCard
          title="Disagreements"
          count={synthesis.synthesis.live_disagreements.length}
          color="#ef4444"
          items={disagreementItems}
        />
        <SynthesisCard
          title="Capitulations"
          count={synthesis.synthesis.flagged_capitulations.length}
          color="#f59e0b"
          items={capitulationItems}
        />
        <SynthesisCard
          title="Minority Positions"
          count={synthesis.synthesis.minority_positions.length}
          color="#8b5cf6"
          items={minorityItems}
        />
      </div>

      <!-- Confidence trajectories -->
      {#if synthesis.synthesis.confidence_trajectories && Object.keys(synthesis.synthesis.confidence_trajectories).length > 0}
        <div class="mb-6">
          <ConfidenceChart trajectories={synthesis.synthesis.confidence_trajectories} />
        </div>
      {/if}

      <!-- Meta observations -->
      {#if synthesis.synthesis.meta_observations}
        <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-4 mb-6">
          <h3 class="text-xs mono text-[var(--text-muted)] mb-2 uppercase tracking-wider">
            Meta Observations
          </h3>
          <p class="text-sm text-[var(--text-secondary)] whitespace-pre-wrap leading-relaxed">
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
            <span class="text-xs mono px-2 py-1 rounded bg-green-500/10 text-green-400 border border-green-500/20">
              All {cc.citations_total} citations valid
            </span>
          {:else}
            <span class="text-xs mono px-2 py-1 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20">
              {cc.citations_invalid.length} invalid citation{cc.citations_invalid.length !== 1 ? 's' : ''} of {cc.citations_total}
            </span>
          {/if}
        </div>
      {/if}

      <RawJsonToggle data={synthesis} />
      <div class="mb-8"></div>

    {:else if !isTerminal}
      <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-6 mb-8 text-center">
        <p class="text-sm text-[var(--text-muted)] mono">
          Synthesis will appear when all rounds complete.
        </p>
      </div>
    {/if}

    <!-- Transcript -->
    {#if transcript}
      <h2 class="text-xs mono text-[var(--text-muted)] uppercase tracking-wider mb-4">
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
  </div>
{/if}
