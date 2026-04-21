<script lang="ts">
  import { api } from '$lib/api/client';
  import StatusBadge from '$lib/components/StatusBadge.svelte';
  import type { DebateResponse } from '$lib/types';
  import { me } from '$lib/stores/me';

  let debates = $state<DebateResponse[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  // Default view is completed debates — that's where the outcome graphs
  // and divergence metrics live, i.e. the reason most visits happen. Put
  // the payoff first, everything else one click away.
  let filter = $state<'complete' | 'running' | 'failed' | 'cancelled' | 'all'>('complete');

  const FILTERS = ['complete', 'running', 'failed', 'cancelled', 'all'] as const;

  function formatDate(iso: string): string {
    const d = new Date(iso);
    return d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' }) +
      ' ' + d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
  }

  function truncate(text: string, len: number): string {
    return text.length > len ? text.slice(0, len) + '...' : text;
  }

  let filtered = $derived(
    filter === 'all' ? debates : debates.filter(d => d.status === filter)
  );

  $effect(() => {
    api.debates.list()
      .then(data => { debates = data; })
      .catch(e => { error = e.message ?? 'Failed to load debates'; })
      .finally(() => { loading = false; });
  });
</script>

<div class="max-w-5xl">
  <div class="flex items-center justify-between mb-8">
    <h1 class="mono text-2xl font-bold">Debates</h1>
    {#if $me?.role === 'admin'}
      <a
        href="/debates/new"
        class="px-4 py-2 bg-[#8b5cf6] text-white rounded-lg text-sm font-medium hover:bg-[#7c3aed] transition-colors no-underline"
      >
        New Debate
      </a>
    {/if}
  </div>

  <!-- Status filters -->
  <div class="flex gap-2 mb-6">
    {#each FILTERS as f}
      <button
        onclick={() => (filter = f)}
        class="px-3 py-1.5 rounded-lg text-xs mono transition-colors {filter === f
          ? 'bg-[#8b5cf6] text-white'
          : 'bg-[var(--surface)] text-[var(--text-secondary)] border border-[var(--border)] hover:text-[var(--text-primary)]'}"
      >
        {f.charAt(0).toUpperCase() + f.slice(1)}
      </button>
    {/each}
  </div>

  <!-- Loading skeleton -->
  {#if loading}
    <div class="space-y-4">
      {#each Array(4) as _}
        <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-5 animate-pulse">
          <div class="h-4 bg-[var(--border)] rounded w-3/4 mb-3"></div>
          <div class="h-3 bg-[var(--border)] rounded w-1/3 mb-2"></div>
          <div class="h-3 bg-[var(--border)] rounded w-1/4"></div>
        </div>
      {/each}
    </div>

  <!-- Error state -->
  {:else if error}
    <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-6 text-center">
      <p class="text-red-400 mono text-sm">{error}</p>
      <button
        onclick={() => { loading = true; error = null; api.debates.list().then(d => { debates = d; }).catch(e => { error = e.message; }).finally(() => { loading = false; }); }}
        class="mt-3 px-4 py-1.5 text-xs mono text-red-400 border border-red-500/30 rounded hover:bg-red-500/10 transition-colors"
      >
        Retry
      </button>
    </div>

  <!-- Empty state -->
  {:else if filtered.length === 0}
    <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-12 text-center">
      {#if filter !== 'all' && debates.length > 0}
        <p class="text-[var(--text-muted)] mono text-sm">No {filter} debates found.</p>
      {:else}
        <p class="text-[var(--text-secondary)] mb-2">No debates yet.</p>
        {#if $me?.role === 'admin'}
          <p class="text-[var(--text-muted)] text-sm mb-4">Create your first debate to get started.</p>
          <a
            href="/debates/new"
            class="inline-block px-4 py-2 bg-[#8b5cf6] text-white rounded-lg text-sm font-medium hover:bg-[#7c3aed] transition-colors no-underline"
          >
            New Debate
          </a>
        {:else}
          <p class="text-[var(--text-muted)] text-sm">Only admins can create debates.</p>
        {/if}
      {/if}
    </div>

  <!-- Debate cards -->
  {:else}
    <div class="space-y-3">
      {#each filtered as debate (debate.id)}
        <a
          href="/debates/{debate.id}"
          class="block bg-[var(--surface)] border border-[var(--border)] rounded-lg p-5 hover:border-[#8b5cf6]/40 transition-colors no-underline group"
        >
          <div class="flex items-start justify-between gap-4">
            <div class="flex-1 min-w-0">
              <h3 class="text-sm font-medium text-[var(--text-primary)] group-hover:text-white transition-colors mb-1.5">
                {truncate(debate.topic, 120)}
              </h3>
              <div class="flex items-center gap-3 flex-wrap">
                <StatusBadge status={debate.status} />
                <span class="text-[10px] mono text-[var(--text-muted)]">
                  {debate.bots.length} agent{debate.bots.length !== 1 ? 's' : ''}
                </span>
                <span class="text-[10px] mono text-[var(--text-muted)]">
                  {debate.id.slice(0, 8)}
                </span>
              </div>
            </div>
            <span class="text-[10px] mono text-[var(--text-muted)] shrink-0">
              {formatDate(debate.created_at)}
            </span>
          </div>
        </a>
      {/each}
    </div>
  {/if}
</div>
