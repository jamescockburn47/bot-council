<script lang="ts">
  import { api, ApiError } from '$lib/api/client';
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
  let showArchived = $state(false);
  // A pending delete confirmation: holds the debate id the admin has
  // clicked "Delete" on. Clicking Delete a second time on the same id
  // commits. Clicking Delete on a different id (or any other action)
  // resets the prompt.
  let confirmingDelete = $state<string | null>(null);
  let busyId = $state<string | null>(null);

  const FILTERS = ['complete', 'running', 'failed', 'cancelled', 'all'] as const;

  function formatDate(iso: string): string {
    const d = new Date(iso);
    return (
      d.toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' }) +
      ' ' +
      d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
    );
  }

  function truncate(text: string, len: number): string {
    return text.length > len ? text.slice(0, len) + '...' : text;
  }

  // "Running" is a UX tab, not a backend status. Map it to every in-flight
  // status so the tab actually matches debates between creation and
  // completion. Backend DebateStatus values are: created, dispatching,
  // scoring, round_0..round_4, analysing, synthesising (all considered
  // running), plus terminal complete / cancelled / failed.
  const RUNNING_STATUSES = new Set([
    'created',
    'dispatching',
    'scoring',
    'round_0',
    'round_1',
    'round_2',
    'round_3',
    'round_4',
    'analysing',
    'synthesising',
  ]);

  let filtered = $derived(
    filter === 'all'
      ? debates
      : filter === 'running'
        ? debates.filter((d) => RUNNING_STATUSES.has(d.status))
        : debates.filter((d) => d.status === filter),
  );

  async function reload() {
    loading = true;
    error = null;
    try {
      debates = await api.debates.list({ archived: showArchived });
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load debates';
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    // Re-run whenever showArchived flips.
    showArchived;
    void reload();
  });

  async function doArchive(d: DebateResponse, archived: boolean) {
    if (busyId) return;
    busyId = d.id;
    confirmingDelete = null;
    try {
      await api.debates.setArchived(d.id, archived);
      // If we just archived it and we're not showing archived, drop the
      // row locally so the UI feels instant without a refetch.
      if (archived && !showArchived) {
        debates = debates.filter((x) => x.id !== d.id);
      } else {
        debates = debates.map((x) =>
          x.id === d.id ? { ...x, archived_at: archived ? new Date().toISOString() : null } : x,
        );
      }
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to archive';
    } finally {
      busyId = null;
    }
  }

  async function doDelete(d: DebateResponse) {
    if (confirmingDelete !== d.id) {
      confirmingDelete = d.id;
      return;
    }
    if (busyId) return;
    busyId = d.id;
    try {
      await api.debates.remove(d.id);
      debates = debates.filter((x) => x.id !== d.id);
      confirmingDelete = null;
    } catch (e) {
      error = e instanceof ApiError ? `Delete failed: ${e.status}` : 'Failed to delete';
    } finally {
      busyId = null;
    }
  }
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

  <!-- Status filters + archived toggle -->
  <div class="flex items-center gap-2 mb-6 flex-wrap">
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
    {#if $me?.role === 'admin'}
      <span class="w-px h-5 bg-[var(--border)] mx-1"></span>
      <label class="flex items-center gap-2 text-xs mono text-[var(--text-secondary)] cursor-pointer select-none">
        <input
          type="checkbox"
          bind:checked={showArchived}
          class="accent-[#8b5cf6]"
        />
        Show archived
      </label>
    {/if}
  </div>

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
  {:else if error}
    <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-6 text-center">
      <p class="text-red-400 mono text-sm">{error}</p>
      <button
        onclick={reload}
        class="mt-3 px-4 py-1.5 text-xs mono text-red-400 border border-red-500/30 rounded hover:bg-red-500/10 transition-colors"
      >
        Retry
      </button>
    </div>
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
  {:else}
    <div class="space-y-3">
      {#each filtered as debate (debate.id)}
        {@const archived = debate.archived_at != null}
        <div
          class="bg-[var(--surface)] border rounded-lg p-5 transition-colors group
                 {archived ? 'border-[var(--border)] opacity-60' : 'border-[var(--border)] hover:border-[#8b5cf6]/40'}"
        >
          <div class="flex items-start justify-between gap-4">
            <a
              href="/debates/{debate.id}"
              class="flex-1 min-w-0 no-underline"
            >
              <h3 class="text-sm font-medium text-[var(--text-primary)] group-hover:text-white transition-colors mb-1.5">
                {truncate(debate.topic, 120)}
                {#if archived}
                  <span class="ml-2 text-[10px] mono uppercase tracking-wider text-[var(--text-muted)] border border-[var(--border)] rounded px-1.5 py-0.5 align-middle">
                    archived
                  </span>
                {/if}
              </h3>
              <div class="flex items-center gap-3 flex-wrap">
                <StatusBadge status={debate.status} />
                <span class="text-[10px] mono text-[var(--text-muted)]">
                  {debate.bots.length} agent{debate.bots.length !== 1 ? 's' : ''}
                </span>
                <span class="text-[10px] mono text-[var(--text-muted)]">
                  {debate.id.slice(0, 8)}
                </span>
                <span class="text-[10px] mono text-[var(--text-muted)]">
                  {formatDate(debate.created_at)}
                </span>
              </div>
            </a>

            {#if $me?.role === 'admin'}
              <div class="flex items-center gap-2 shrink-0">
                <button
                  type="button"
                  disabled={busyId === debate.id}
                  onclick={() => doArchive(debate, !archived)}
                  class="px-2.5 py-1 text-[10px] mono text-[var(--text-secondary)] border border-[var(--border)] rounded hover:text-[var(--text-primary)] hover:border-[var(--text-muted)] transition-colors disabled:opacity-50"
                >
                  {archived ? 'Unarchive' : 'Archive'}
                </button>
                <button
                  type="button"
                  disabled={busyId === debate.id}
                  onclick={() => doDelete(debate)}
                  class="px-2.5 py-1 text-[10px] mono rounded border transition-colors disabled:opacity-50
                         {confirmingDelete === debate.id
                           ? 'text-red-300 bg-red-500/20 border-red-500/50'
                           : 'text-[var(--text-muted)] border-[var(--border)] hover:text-red-400 hover:border-red-500/40'}"
                  title="Permanent delete (cascade)"
                >
                  {confirmingDelete === debate.id ? 'Click to confirm' : 'Delete'}
                </button>
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
