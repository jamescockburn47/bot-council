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
  <div class="flex items-end justify-between mb-8 flex-wrap gap-4">
    <div>
      <p class="tm-eyebrow mb-2" style="color: var(--indigo-400);">Workspace</p>
      <h1 style="font-family: var(--sans-product); font-weight: 800; font-size: 32px; letter-spacing: -0.02em; color: var(--glow-txt);">
        Debates
        <span class="stat-serif" style="font-size: 28px; margin-left: 12px;">{debates.length}</span>
      </h1>
    </div>
    {#if $me?.role === 'admin'}
      <a href="/debates/new" class="btn-indigo no-underline">New Debate →</a>
    {/if}
  </div>

  <!-- Status filters + archived toggle -->
  <div class="flex items-center gap-2 mb-6 flex-wrap">
    {#each FILTERS as f}
      <button
        onclick={() => (filter = f)}
        class={filter === f ? 'pill-on' : 'pill-off'}
      >
        {f.charAt(0).toUpperCase() + f.slice(1)}
      </button>
    {/each}
    {#if $me?.role === 'admin'}
      <span style="width: 1px; height: 20px; background: var(--night-rule); margin: 0 4px;"></span>
      <label
        class="flex items-center gap-2 cursor-pointer select-none"
        style="font-family: var(--mono-product); font-size: 12px; color: var(--glow-mute);"
      >
        <input
          type="checkbox"
          bind:checked={showArchived}
          style="accent-color: var(--indigo-500);"
        />
        Show archived
      </label>
    {/if}
  </div>

  {#if loading}
    <div class="space-y-4">
      {#each Array(4) as _}
        <div class="card-term animate-pulse">
          <div class="h-4 rounded w-3/4 mb-3" style="background: var(--night-rule);"></div>
          <div class="h-3 rounded w-1/3 mb-2" style="background: var(--night-rule);"></div>
          <div class="h-3 rounded w-1/4" style="background: var(--night-rule);"></div>
        </div>
      {/each}
    </div>
  {:else if error}
    <div style="background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--r-lg); padding: 20px; text-align: center;">
      <p style="font-family: var(--mono-product); font-size: 13px; color: #FCA5A5;">{error}</p>
      <button
        onclick={reload}
        style="margin-top: 12px; font-family: var(--mono-product); font-size: 11px; padding: 6px 14px; color: #FCA5A5; border: 1px solid rgba(239,68,68,0.3); border-radius: 8px; background: transparent; cursor: pointer;"
      >
        Retry
      </button>
    </div>
  {:else if filtered.length === 0}
    <div class="card-term" style="padding: 48px; text-align: center;">
      {#if filter !== 'all' && debates.length > 0}
        <p style="font-family: var(--mono-product); font-size: 13px; color: var(--glow-mute);">No {filter} debates found.</p>
      {:else}
        <p style="font-family: var(--sans-product); font-size: 16px; color: var(--glow-dim); margin-bottom: 8px;">No debates yet.</p>
        {#if $me?.role === 'admin'}
          <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-mute); margin-bottom: 16px;">Create your first debate to get started.</p>
          <a href="/debates/new" class="btn-indigo no-underline" style="display: inline-block;">New Debate →</a>
        {:else}
          <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-mute); margin-bottom: 16px;">Only admins can create debates.</p>
        {/if}
      {/if}
    </div>
  {:else}
    <div class="space-y-3">
      {#each filtered as debate (debate.id)}
        {@const archived = debate.archived_at != null}
        <div class="card-term card-term-hover" style="padding: 18px; opacity: {archived ? 0.55 : 1};">
          <div class="flex items-start justify-between gap-4">
            <a
              href="/debates/{debate.id}"
              class="flex-1 min-w-0 no-underline"
            >
              <h3 style="font-family: var(--sans-product); font-weight: 600; font-size: 15px; color: var(--glow-txt); line-height: 1.35; margin-bottom: 8px;">
                {truncate(debate.topic, 120)}
                {#if archived}
                  <span class="pill-on" style="background: rgba(154,52,18,0.14); color: var(--copper); border-color: rgba(154,52,18,0.35); font-size: 9px; padding: 2px 8px; margin-left: 8px; vertical-align: middle;">
                    archived
                  </span>
                {/if}
              </h3>
              <div class="flex items-center gap-3 flex-wrap">
                <StatusBadge status={debate.status} />
                <span class="stat-serif" style="font-size: 16px;">{debate.bots.length}</span>
                <span class="mono-label" style="color: var(--glow-faint); font-size: 10px; letter-spacing: 0.15em;">agent{debate.bots.length !== 1 ? 's' : ''}</span>
                <span class="mono-label" style="color: var(--glow-faint); font-size: 10px; letter-spacing: 0.15em;">
                  {debate.id.slice(0, 8)}
                </span>
                <span class="mono-label" style="color: var(--glow-faint); font-size: 10px; letter-spacing: 0.15em;">
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
                  class="btn-dark-ghost"
                  style="font-size: 10px; padding: 5px 12px;"
                >
                  {archived ? 'Unarchive' : 'Archive'}
                </button>
                <button
                  type="button"
                  disabled={busyId === debate.id}
                  onclick={() => doDelete(debate)}
                  title="Permanent delete (cascade)"
                  class={confirmingDelete === debate.id ? '' : 'btn-dark-ghost'}
                  style={confirmingDelete === debate.id
                    ? 'font-size: 10px; padding: 5px 12px; background: rgba(239,68,68,0.15); color: #FCA5A5; border: 1px solid rgba(239,68,68,0.5); border-radius: 8px; font-family: var(--sans-product); font-weight: 500; cursor: pointer;'
                    : 'font-size: 10px; padding: 5px 12px;'}
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
