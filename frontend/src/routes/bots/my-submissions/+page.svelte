<script lang="ts">
  import { api } from '$lib/api/client';
  import StatusBadge from '$lib/components/StatusBadge.svelte';
  import type { BotResponse } from '$lib/types';

  let submissions = $state<BotResponse[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleDateString('en-GB', {
      day: 'numeric',
      month: 'short',
      year: 'numeric',
    });
  }

  $effect(() => {
    api.bots
      .mySubmissions()
      .then(data => {
        submissions = data;
      })
      .catch(e => {
        error = e instanceof Error ? e.message : 'Failed to load submissions';
      })
      .finally(() => {
        loading = false;
      });
  });
</script>

<div class="max-w-4xl">
  <div class="mb-8">
    <a
      href="/bots"
      class="text-xs mono text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors no-underline"
    >
      &larr; Back to bots
    </a>
    <h1 class="mono text-2xl font-bold mt-2">My Submissions</h1>
  </div>

  {#if loading}
    <div class="space-y-3">
      {#each Array(3) as _}
        <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-5 animate-pulse">
          <div class="h-4 bg-[var(--border)] rounded w-1/3 mb-3"></div>
          <div class="h-3 bg-[var(--border)] rounded w-1/2"></div>
        </div>
      {/each}
    </div>
  {:else if error}
    <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-6 text-center">
      <p class="text-red-400 mono text-sm">{error}</p>
    </div>
  {:else if submissions.length === 0}
    <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-12 text-center">
      <p class="text-[var(--text-secondary)] mb-2">No submissions yet.</p>
      <p class="text-[var(--text-muted)] text-sm mb-4">Submit your first bot to participate in debates.</p>
      <a
        href="/bots/submit"
        class="inline-block px-4 py-2 bg-[#8b5cf6] text-white rounded-lg text-sm font-medium hover:bg-[#7c3aed] transition-colors no-underline"
      >
        Submit a Bot
      </a>
    </div>
  {:else}
    <div class="space-y-3">
      {#each submissions as bot (bot.id)}
        <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-5">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-3">
              <span class="text-sm font-medium text-[var(--text-primary)]">{bot.name}</span>
              <StatusBadge status={bot.status} />
              {#if bot.model_family}
                <span class="text-[10px] mono text-[var(--text-muted)] px-1.5 py-0.5 bg-[var(--border)] rounded">
                  {bot.model_family}
                </span>
              {/if}
            </div>
            <span class="text-xs text-[var(--text-muted)]">{formatDate(bot.created_at)}</span>
          </div>
          <p class="text-xs mono text-[var(--text-muted)] mt-1.5">{bot.endpoint_url}</p>
          {#if bot.rejection_reason && (bot.status === 'rejected' || bot.status === 'smoke_test_failed')}
            <div class="mt-3 bg-red-500/10 border border-red-500/30 rounded-md p-3">
              <div class="mono text-xs text-red-400 uppercase tracking-wider mb-1">
                {bot.status === 'rejected' ? 'Rejected' : 'Smoke test failed'}
              </div>
              <p class="text-sm text-[var(--text-secondary)]">{bot.rejection_reason}</p>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
