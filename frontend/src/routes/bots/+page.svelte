<script lang="ts">
  import { api, ApiError } from '$lib/api/client';
  import StatusBadge from '$lib/components/StatusBadge.svelte';
  import type { BotResponse } from '$lib/types';

  let bots = $state<BotResponse[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let tab = $state<'active' | 'pending' | 'inactive'>('active');
  let actionLoading = $state<string | null>(null);

  let active = $derived(bots.filter(b => b.status === 'active'));
  let pending = $derived(bots.filter(b => b.status === 'pending'));
  let inactive = $derived(bots.filter(b => b.status === 'inactive' || b.status === 'rejected'));

  const TABS = [
    { key: 'active' as const, label: 'Active', count: () => active.length },
    { key: 'pending' as const, label: 'Pending', count: () => pending.length },
    { key: 'inactive' as const, label: 'Inactive', count: () => inactive.length },
  ];

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleDateString('en-GB', {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    });
  }

  async function loadBots() {
    loading = true;
    error = null;
    try {
      bots = await api.bots.list();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load bots';
    } finally {
      loading = false;
    }
  }

  async function handleAction(action: 'approve' | 'reject' | 'deactivate' | 'reactivate', id: string) {
    actionLoading = id;
    try {
      await api.bots[action](id);
      await loadBots();
    } catch (e) {
      const msg = e instanceof ApiError ? `Error ${e.status}` : 'Action failed';
      error = msg;
    } finally {
      actionLoading = null;
    }
  }

  $effect(() => {
    loadBots();
  });
</script>

<div class="max-w-5xl">
  <div class="flex items-center justify-between mb-8">
    <h1 class="mono text-2xl font-bold">Bot Management</h1>
    <div class="flex gap-2">
      <a
        href="/bots/submit"
        class="px-4 py-2 bg-[#8b5cf6] text-white rounded-lg text-sm font-medium hover:bg-[#7c3aed] transition-colors no-underline"
      >
        Submit Bot
      </a>
      <a
        href="/bots/my-submissions"
        class="px-4 py-2 bg-[var(--surface)] text-[var(--text-secondary)] border border-[var(--border)] rounded-lg text-sm hover:text-[var(--text-primary)] transition-colors no-underline"
      >
        My Submissions
      </a>
    </div>
  </div>

  <!-- Tabs -->
  <div class="flex gap-1 mb-6 border-b border-[var(--border)]">
    {#each TABS as t}
      <button
        onclick={() => { tab = t.key; }}
        class="px-4 py-2.5 text-sm mono transition-colors relative {tab === t.key
          ? 'text-[var(--text-primary)]'
          : 'text-[var(--text-muted)] hover:text-[var(--text-secondary)]'}"
      >
        {t.label}
        <span
          class="ml-1.5 text-[10px] px-1.5 py-0.5 rounded-full {tab === t.key
            ? 'bg-[#8b5cf6]/20 text-[#8b5cf6]'
            : 'bg-[var(--border)] text-[var(--text-muted)]'}"
        >
          {t.count()}
        </span>
        {#if tab === t.key}
          <div class="absolute bottom-0 left-0 right-0 h-0.5 bg-[#8b5cf6]"></div>
        {/if}
      </button>
    {/each}
  </div>

  <!-- Loading -->
  {#if loading}
    <div class="space-y-3">
      {#each Array(3) as _}
        <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-5 animate-pulse">
          <div class="h-4 bg-[var(--border)] rounded w-1/3 mb-3"></div>
          <div class="h-3 bg-[var(--border)] rounded w-1/2"></div>
        </div>
      {/each}
    </div>

  <!-- Error -->
  {:else if error}
    <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-6 text-center">
      <p class="text-red-400 mono text-sm">{error}</p>
      <button
        onclick={loadBots}
        class="mt-3 px-4 py-1.5 text-xs mono text-red-400 border border-red-500/30 rounded hover:bg-red-500/10 transition-colors"
      >
        Retry
      </button>
    </div>

  <!-- Active Tab -->
  {:else if tab === 'active'}
    {#if active.length === 0}
      <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-8 text-center text-[var(--text-muted)] text-sm">
        No active bots.
      </div>
    {:else}
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b border-[var(--border)]">
              <th class="text-left py-3 px-4 text-xs mono text-[var(--text-muted)] font-normal">Name</th>
              <th class="text-left py-3 px-4 text-xs mono text-[var(--text-muted)] font-normal">Endpoint</th>
              <th class="text-left py-3 px-4 text-xs mono text-[var(--text-muted)] font-normal">Model</th>
              <th class="text-left py-3 px-4 text-xs mono text-[var(--text-muted)] font-normal">Added</th>
              <th class="text-right py-3 px-4 text-xs mono text-[var(--text-muted)] font-normal">Action</th>
            </tr>
          </thead>
          <tbody>
            {#each active as bot (bot.id)}
              <tr class="border-b border-[var(--border)] last:border-0 hover:bg-[rgba(255,255,255,0.02)]">
                <td class="py-3 px-4 text-[var(--text-primary)]">{bot.name}</td>
                <td class="py-3 px-4 mono text-xs text-[var(--text-muted)] max-w-48 truncate">{bot.endpoint_url}</td>
                <td class="py-3 px-4">
                  {#if bot.model_family}
                    <span class="text-[10px] mono text-[var(--text-muted)] px-1.5 py-0.5 bg-[var(--border)] rounded">
                      {bot.model_family}
                    </span>
                  {:else}
                    <span class="text-[var(--text-muted)]">&mdash;</span>
                  {/if}
                </td>
                <td class="py-3 px-4 text-xs text-[var(--text-muted)]">{formatDate(bot.created_at)}</td>
                <td class="py-3 px-4 text-right">
                  <button
                    onclick={() => handleAction('deactivate', bot.id)}
                    disabled={actionLoading === bot.id}
                    class="px-3 py-1 text-xs mono text-amber-400 border border-amber-500/30 rounded hover:bg-amber-500/10 transition-colors disabled:opacity-50"
                  >
                    {actionLoading === bot.id ? '...' : 'Deactivate'}
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}

  <!-- Pending Tab -->
  {:else if tab === 'pending'}
    {#if pending.length === 0}
      <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-8 text-center text-[var(--text-muted)] text-sm">
        No pending submissions.
      </div>
    {:else}
      <div class="space-y-4">
        {#each pending as bot (bot.id)}
          <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-5">
            <div class="flex items-start justify-between gap-4">
              <div class="flex-1">
                <div class="flex items-center gap-3 mb-2">
                  <h3 class="text-sm font-medium text-[var(--text-primary)]">{bot.name}</h3>
                  <StatusBadge status={bot.status} />
                  {#if bot.model_family}
                    <span class="text-[10px] mono text-[var(--text-muted)] px-1.5 py-0.5 bg-[var(--border)] rounded">
                      {bot.model_family}
                    </span>
                  {/if}
                </div>
                <div class="space-y-1 text-xs text-[var(--text-muted)]">
                  <p>
                    <span class="mono">Endpoint:</span>
                    <span class="text-[var(--text-secondary)]">{bot.endpoint_url}</span>
                  </p>
                  {#if bot.description}
                    <p>
                      <span class="mono">Description:</span>
                      <span class="text-[var(--text-secondary)]">{bot.description}</span>
                    </p>
                  {/if}
                  {#if bot.submitted_by}
                    <p>
                      <span class="mono">Submitted by:</span>
                      <span class="text-[var(--text-secondary)]">{bot.submitted_by}</span>
                    </p>
                  {/if}
                  <p>
                    <span class="mono">Date:</span>
                    <span class="text-[var(--text-secondary)]">{formatDate(bot.created_at)}</span>
                  </p>
                </div>
              </div>
              <div class="flex gap-2 shrink-0">
                <button
                  onclick={() => handleAction('approve', bot.id)}
                  disabled={actionLoading === bot.id}
                  class="px-3 py-1.5 text-xs mono text-green-400 border border-green-500/30 rounded hover:bg-green-500/10 transition-colors disabled:opacity-50"
                >
                  {actionLoading === bot.id ? '...' : 'Approve'}
                </button>
                <button
                  onclick={() => handleAction('reject', bot.id)}
                  disabled={actionLoading === bot.id}
                  class="px-3 py-1.5 text-xs mono text-red-400 border border-red-500/30 rounded hover:bg-red-500/10 transition-colors disabled:opacity-50"
                >
                  {actionLoading === bot.id ? '...' : 'Reject'}
                </button>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}

  <!-- Inactive Tab -->
  {:else if tab === 'inactive'}
    {#if inactive.length === 0}
      <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-8 text-center text-[var(--text-muted)] text-sm">
        No inactive bots.
      </div>
    {:else}
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b border-[var(--border)]">
              <th class="text-left py-3 px-4 text-xs mono text-[var(--text-muted)] font-normal">Name</th>
              <th class="text-left py-3 px-4 text-xs mono text-[var(--text-muted)] font-normal">Endpoint</th>
              <th class="text-left py-3 px-4 text-xs mono text-[var(--text-muted)] font-normal">Model</th>
              <th class="text-left py-3 px-4 text-xs mono text-[var(--text-muted)] font-normal">Status</th>
              <th class="text-right py-3 px-4 text-xs mono text-[var(--text-muted)] font-normal">Action</th>
            </tr>
          </thead>
          <tbody>
            {#each inactive as bot (bot.id)}
              <tr class="border-b border-[var(--border)] last:border-0 hover:bg-[rgba(255,255,255,0.02)]">
                <td class="py-3 px-4 text-[var(--text-primary)]">{bot.name}</td>
                <td class="py-3 px-4 mono text-xs text-[var(--text-muted)] max-w-48 truncate">{bot.endpoint_url}</td>
                <td class="py-3 px-4">
                  {#if bot.model_family}
                    <span class="text-[10px] mono text-[var(--text-muted)] px-1.5 py-0.5 bg-[var(--border)] rounded">
                      {bot.model_family}
                    </span>
                  {:else}
                    <span class="text-[var(--text-muted)]">&mdash;</span>
                  {/if}
                </td>
                <td class="py-3 px-4"><StatusBadge status={bot.status} /></td>
                <td class="py-3 px-4 text-right">
                  <button
                    onclick={() => handleAction('reactivate', bot.id)}
                    disabled={actionLoading === bot.id}
                    class="px-3 py-1 text-xs mono text-green-400 border border-green-500/30 rounded hover:bg-green-500/10 transition-colors disabled:opacity-50"
                  >
                    {actionLoading === bot.id ? '...' : 'Reactivate'}
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</div>
