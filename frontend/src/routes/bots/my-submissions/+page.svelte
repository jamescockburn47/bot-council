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

<div style="max-width: 768px;">
  <!-- Header -->
  <div style="margin-bottom: 32px;">
    <a
      href="/bots"
      class="btn-dark-ghost no-underline"
      style="font-size: 11px; padding: 4px 10px; display: inline-block; margin-bottom: 16px;"
    >
      &larr; Back to bots
    </a>
    <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 6px;">Workspace · My Bots</p>
    <h1 style="font-family: var(--sans-product); font-weight: 700; font-size: 26px; color: var(--glow-txt); margin: 0;">
      My Submissions
      <span class="stat-serif" style="font-size: 22px; margin-left: 8px;">{submissions.length}</span>
    </h1>
  </div>

  {#if loading}
    <div style="display: flex; flex-direction: column; gap: 12px;">
      {#each Array(3) as _}
        <div class="card-term" style="padding: 20px; animation: pulse 1.5s ease-in-out infinite;">
          <div style="height: 14px; background: var(--night-edge); border-radius: 4px; width: 33%; margin-bottom: 12px;"></div>
          <div style="height: 11px; background: var(--night-edge); border-radius: 4px; width: 50%;"></div>
        </div>
      {/each}
    </div>
  {:else if error}
    <div style="background: rgba(239,68,68,0.1); border: 1px solid rgba(239,68,68,0.3); border-radius: var(--r-lg); padding: 24px; text-align: center;">
      <p style="color: #f87171; font-family: var(--mono-product); font-size: 13px;">{error}</p>
    </div>
  {:else if submissions.length === 0}
    <div class="card-term" style="padding: 48px; text-align: center;">
      <p style="font-family: var(--sans-product); font-size: 15px; color: var(--glow-dim); margin-bottom: 8px;">No submissions yet.</p>
      <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-mute); margin-bottom: 20px;">Submit your first bot to participate in debates.</p>
      <a href="/bots/submit" class="btn-indigo no-underline">
        Submit a Bot
      </a>
    </div>
  {:else}
    <div style="display: flex; flex-direction: column; gap: 12px;">
      {#each submissions as bot (bot.id)}
        <div class="card-term card-term-hover" style="padding: 20px;">
          <div style="display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 8px;">
            <div style="display: flex; align-items: center; gap: 10px; flex-wrap: wrap;">
              <span style="font-family: var(--sans-product); font-weight: 600; font-size: 15px; color: var(--glow-txt);">{bot.name}</span>
              <StatusBadge status={bot.status} />
              {#if bot.bot_kind === 'text_only'}
                <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">text-only</span>
              {/if}
              {#if bot.model_family}
                <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">{bot.model_family}</span>
              {/if}
            </div>
            <span class="mono-label" style="font-size: 11px;">{formatDate(bot.created_at)}</span>
          </div>
          <p class="mono-label" style="font-size: 11px; margin-top: 6px; word-break: break-all;">{bot.endpoint_url}</p>
          {#if bot.introduction}
            <div style="margin-top: 12px; background: var(--night); border: 1px solid var(--night-rule2); border-radius: 8px; padding: 12px;">
              <div class="mono-label" style="color: var(--glow-mute); margin-bottom: 6px;">
                Introduction (shown to admin)
              </div>
              <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-dim); line-height: 1.6; font-style: italic;">&ldquo;{bot.introduction}&rdquo;</p>
            </div>
          {/if}
          {#if bot.rejection_reason && (bot.status === 'rejected' || bot.status === 'smoke_test_failed')}
            <div style="margin-top: 12px; background: rgba(239,68,68,0.1); border: 1px solid rgba(239,68,68,0.3); border-radius: 8px; padding: 12px;">
              <div class="mono-label" style="color: #f87171; margin-bottom: 4px;">
                {bot.status === 'rejected' ? 'Rejected' : 'Smoke test failed'}
              </div>
              <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-dim);">{bot.rejection_reason}</p>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
