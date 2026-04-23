<script lang="ts">
  import * as Sentry from '@sentry/browser';
  import { api, debateStreamUrl } from '$lib/api/client';
  import { getSessionToken } from '$lib/auth/clerk';
  import StatusBadge from '$lib/components/StatusBadge.svelte';
  import TabBar from '$lib/components/TabBar.svelte';
  import DebateTranscriptView from '$lib/components/DebateTranscriptView.svelte';
  import RawJsonToggle from '$lib/components/RawJsonToggle.svelte';
  import OutcomeTab from '$lib/components/outcome/OutcomeTab.svelte';
  import type {
    DebateResponse,
    TranscriptResponse,
    SynthesisResponse,
  } from '$lib/types';

  type Tab = 'outcome' | 'transcript' | 'raw';

  let { data } = $props();

  let debate = $state<DebateResponse | null>(null);
  let transcript = $state<TranscriptResponse | null>(null);
  let synthesis = $state<SynthesisResponse | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let sseConnected = $state(false);

  const TERMINAL = ['complete', 'cancelled', 'failed'];

  function formatDate(iso: string): string {
    const d = new Date(iso);
    return (
      d.toLocaleDateString('en-GB', {
        day: 'numeric',
        month: 'short',
        year: 'numeric',
      }) +
      ' ' +
      d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
    );
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

  // Read the ?tab= query once at mount via window.location to avoid the
  // $app/stores `page` subscription entirely. The reactive $page pattern
  // has been the source of the "Cannot read properties of null (reading
  // 'r')" Svelte 5 runtime crash — the page store's value can be null
  // during early static-adapter hydration. setTab() below uses history
  // API directly so we never depend on reactive URL updates here.
  function readInitialTab(): Tab | null {
    if (typeof window === 'undefined') return null;
    try {
      const p = new URLSearchParams(window.location.search).get('tab');
      if (p === 'outcome' || p === 'transcript' || p === 'raw') return p;
    } catch {
      // ignore
    }
    return null;
  }

  let explicitTab = $state<Tab | null>(readInitialTab());

  let activeTab = $derived<Tab>(
    explicitTab ?? (isTerminal ? 'outcome' : 'transcript'),
  );

  let tabs = $derived([
    {
      id: 'outcome',
      label: 'Outcome',
      disabled: !isTerminal,
    },
    { id: 'transcript', label: 'Transcript' },
    { id: 'raw', label: 'Raw' },
  ]);

  function setTab(id: string) {
    const tab = id as Tab;
    explicitTab = tab;
    if (typeof window === 'undefined') return;
    try {
      const url = new URL(window.location.href);
      url.searchParams.set('tab', tab);
      window.history.replaceState(window.history.state, '', url.toString());
    } catch {
      // history API can throw in rare cases; tab selection still updates in-memory
    }
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

  // EventSource for live debate updates.
  // EventSource cannot set Authorization headers, so the Clerk session token
  // is passed via ?token=<jwt> query param. The backend's authenticate()
  // falls back to this when no Authorization header is present.
  $effect(() => {
    if (!debate || isTerminalStatus(debate.status)) return;

    let es: EventSource | null = null;
    let cancelled = false;

    (async () => {
      const token = await getSessionToken().catch(() => null);
      if (cancelled) return;
      es = new EventSource(debateStreamUrl(data.debateId, token));

      es.onopen = () => {
        sseConnected = true;
      };
      es.onerror = () => {
        sseConnected = false;
      };

      es.addEventListener('round:started', (e: MessageEvent) => {
        const d = JSON.parse(e.data);
        if (transcript) {
          const exists = transcript.rounds.find(
            (r) => r.round_number === d.round_number,
          );
          if (!exists) {
            transcript.rounds = [
              ...transcript.rounds,
              {
                round_number: d.round_number,
                status: 'in_progress',
                responses: [],
              },
            ];
          }
        }
      });

      es.addEventListener('response:received', (e: MessageEvent) => {
        const d = JSON.parse(e.data);
        if (transcript) {
          const round = transcript.rounds.find(
            (r) => r.round_number === d.round_number,
          );
          if (round) {
            round.responses = [
              ...round.responses,
              {
                pseudonym: d.pseudonym,
                response: d.response,
                confidence: d.confidence ?? null,
                challenge: d.challenge ?? null,
                position_change: d.position_change ?? null,
                valid: d.valid,
                abstained: d.abstained,
                validation_reasoning: null,
                extraction_metadata: d.extraction_metadata ?? null,
                retry_count: d.retry_count ?? 0,
              },
            ];
            transcript = transcript;
          }
        }
      });

      es.addEventListener('round:completed', (e: MessageEvent) => {
        const d = JSON.parse(e.data);
        if (transcript) {
          const round = transcript.rounds.find(
            (r) => r.round_number === d.round_number,
          );
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
        es?.close();
      });

      es.addEventListener('debate:failed', (e: MessageEvent) => {
        const d = JSON.parse(e.data);
        if (debate) debate = { ...debate, status: 'failed' };
        error = `Debate failed: ${d.reason}`;
        sseConnected = false;
        es?.close();
      });
    })();

    return () => {
      cancelled = true;
      es?.close();
      sseConnected = false;
    };
  });

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
    if (debate.completed_at)
      lines.push(`**Completed:** ${formatDate(debate.completed_at)}  `);
    lines.push('');

    lines.push('## Participants');
    lines.push('');
    lines.push('| Agent | Role |');
    lines.push('|-------|------|');
    for (const entry of transcript.anonymisation_log) {
      const bot = debate.bots.find((b) => b.pseudonym === entry.pseudonym);
      const name = bot?.bot_name ?? entry.pseudonym;
      lines.push(`| ${name} (${entry.pseudonym}) | ${entry.role ?? 'none'} |`);
    }
    lines.push('');

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
          lines.push(
            `**${c.bot}** ${c.justification_adequate ? '(justified)' : '(unjustified)'}`,
          );
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
          lines.push(
            `> **Challenge** (${resp.challenge.type}): ${resp.challenge.claim_targeted}`,
          );
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

    if (transcript.divergence_analyses.length > 0) {
      lines.push('---');
      lines.push('');
      lines.push('## Divergence Analysis');
      lines.push('');
      lines.push('| Agent | Shifted | Magnitude | Justified |');
      lines.push('|-------|---------|-----------|-----------|');
      for (const d of transcript.divergence_analyses) {
        lines.push(
          `| ${d.pseudonym} | ${d.shifted ? 'Yes' : 'No'} | ${d.magnitude ?? '-'} | ${d.justification_adequate ? 'Yes' : 'No'} |`,
        );
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
    const slug = debate!.topic
      .slice(0, 40)
      .replace(/[^a-zA-Z0-9]+/g, '-')
      .toLowerCase();
    a.download = `debate-${slug}-${debate!.id.slice(0, 8)}.md`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<svelte:boundary
  onerror={(err, _reset) => {
    console.error('[debate-detail] render boundary caught', err);
    Sentry.captureException(err, {
      tags: { component: 'debates/[id]/+page' },
      extra: { debateId: data.debateId, loading, hasDebate: !!debate, hasTranscript: !!transcript, hasSynthesis: !!synthesis },
    });
  }}
>
{#if loading}
  <div class="max-w-5xl space-y-4">
    <div class="animate-pulse">
      <div class="h-6 rounded w-1/4 mb-4" style="background: var(--night-rule);"></div>
      <div class="h-8 rounded w-3/4 mb-6" style="background: var(--night-rule);"></div>
      <div class="grid grid-cols-2 gap-4">
        {#each Array(4) as _}
          <div class="card-term h-32"></div>
        {/each}
      </div>
    </div>
  </div>
{:else if error}
  <div class="max-w-5xl">
    <div
      class="bg-red-500/10 border border-red-500/30 rounded-lg p-6 text-center"
    >
      <p class="text-red-400 mono text-sm">{error}</p>
      <a
        href="/debates"
        class="inline-block mt-3 px-4 py-1.5 text-xs mono border border-[var(--night-rule)] rounded hover:border-[var(--night-rule3)] transition-colors no-underline"
        style="color: var(--glow-dim);"
      >
        Back to debates
      </a>
    </div>
  </div>
{:else if debate}
  <div class="max-w-5xl">
    <!-- Header -->
    <div class="mb-6">
      <div class="flex items-center gap-3 mb-2">
        <p class="tm-eyebrow" style="color: var(--indigo-400);">Debate</p>
        <a
          href="/debates"
          class="mono-label no-underline"
          style="color: var(--glow-faint);"
        >
          All debates
        </a>
        <span style="color: var(--glow-faint);">/</span>
        <span class="mono-label">{debate.id.slice(0, 8)}</span>
        <StatusBadge status={debate.status} />
        {#if sseConnected}
          <span
            class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs mono"
            style="color: #22c55e; background: #22c55e15; border: 1px solid #22c55e30;"
          >
            <span class="w-1.5 h-1.5 rounded-full bg-[#22c55e] animate-pulse"></span>
            LIVE
          </span>
        {/if}
      </div>
      <h1
        style="font-family: var(--serif); font-weight: 600; font-size: clamp(22px, 3vw, 32px); color: var(--glow-txt); margin-bottom: 12px; line-height: 1.2;"
      >
        {debate.topic}
      </h1>
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-4">
          <span class="mono-label">{debate.bots.length} agent{debate.bots.length !== 1 ? 's' : ''}</span>
          {#if transcript}
            <span class="mono-label">{transcript.rounds.length} round{transcript.rounds.length !== 1 ? 's' : ''}</span>
          {/if}
          <span class="mono-label">Created {formatDate(debate.created_at)}</span>
          {#if debate.completed_at}
            <span class="mono-label">Completed {formatDate(debate.completed_at)}</span>
          {/if}
        </div>
        {#if isTerminal && transcript}
          <button
            onclick={exportMarkdown}
            class="px-3 py-1.5 text-xs mono border rounded transition-colors"
            style="color: var(--glow-dim); border-color: var(--night-rule);"
            onmouseenter={(e) => (e.currentTarget.style.borderColor = 'var(--night-rule3)')}
            onmouseleave={(e) => (e.currentTarget.style.borderColor = 'var(--night-rule)')}
          >
            Export .md
          </button>
        {/if}
      </div>
    </div>

    <TabBar {tabs} active={activeTab} onChange={setTab} />

    {#if activeTab === 'outcome'}
      <OutcomeTab {debate} {synthesis} {transcript} />
    {:else if activeTab === 'raw'}
      <RawJsonToggle data={synthesis ?? debate} />
    {:else}
      <DebateTranscriptView {debate} {synthesis} {transcript} />
    {/if}
  </div>
{/if}

{#snippet failed(err: unknown, reset: () => void)}
  <div class="max-w-3xl mx-auto mt-12">
    <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-6">
      <h2 class="text-sm font-semibold text-red-400 mb-2 mono">Render error</h2>
      <p class="text-xs mb-3 mono whitespace-pre-wrap" style="color: var(--glow-dim);">{err instanceof Error ? err.message : String(err)}</p>
      <p class="text-xs mb-4" style="color: var(--glow-faint);">Reported to Sentry. Try reset, reload, or go back to the debate list.</p>
      <div class="flex gap-3">
        <button onclick={reset} class="btn-indigo text-xs" style="padding: 6px 12px; font-size: 12px;">Reset</button>
        <button onclick={() => location.reload()} class="text-xs mono px-3 py-1.5 border rounded" style="color: var(--glow-dim); border-color: var(--night-rule);">Reload</button>
        <a href="/debates" class="text-xs mono px-3 py-1.5 border rounded no-underline" style="color: var(--glow-dim); border-color: var(--night-rule);">Back</a>
      </div>
    </div>
  </div>
{/snippet}
</svelte:boundary>
