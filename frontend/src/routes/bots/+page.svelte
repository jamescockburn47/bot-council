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
  let pending = $derived(bots.filter(b => b.status === 'pending' || b.status === 'smoke_test_failed'));
  let inactive = $derived(bots.filter(b => b.status === 'inactive' || b.status === 'rejected'));

  // Reject modal state.
  let rejectingBot = $state<BotResponse | null>(null);
  let rejectReason = $state('');
  let submittingReject = $state(false);

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

  async function handleAction(action: 'approve' | 'deactivate' | 'reactivate', id: string) {
    actionLoading = id;
    try {
      await api.bots[action](id);
      await loadBots();
    } catch (e) {
      const msg = e instanceof ApiError
        ? `Error ${e.status}: ${JSON.stringify(e.body)}`
        : 'Action failed';
      error = msg;
    } finally {
      actionLoading = null;
    }
  }

  async function confirmReject() {
    if (!rejectingBot) return;
    const reason = rejectReason.trim();
    if (reason.length < 10) return;
    submittingReject = true;
    try {
      await api.bots.reject(rejectingBot.id, reason);
      await loadBots();
      rejectingBot = null;
      rejectReason = '';
    } catch (e) {
      error = e instanceof ApiError ? `Error ${e.status}: ${JSON.stringify(e.body)}` : 'Reject failed';
    } finally {
      submittingReject = false;
    }
  }

  $effect(() => {
    loadBots();
  });
</script>

<div class="max-w-5xl">
  <!-- Header -->
  <div class="flex items-center justify-between mb-8">
    <div>
      <p class="tm-eyebrow mb-1" style="color: var(--indigo-400);">Workspace · Agents</p>
      <h1 class="page-title" style="font-family: var(--sans-product); font-weight: 700; font-size: 26px; color: var(--glow-txt);">
        Bots
        <span class="stat-serif" style="font-size: 22px; margin-left: 8px;">{bots.length}</span>
      </h1>
    </div>
    <div class="flex gap-2">
      <a href="/bots/submit" class="btn-indigo no-underline">Submit a bot</a>
      <a href="/bots/my-submissions" class="btn-dark-ghost no-underline">My Submissions</a>
    </div>
  </div>

  <!-- Tabs / filter pills -->
  <div class="flex gap-2 mb-6" style="border-bottom: 1px solid var(--night-rule2); padding-bottom: 12px;">
    {#each TABS as t}
      <button
        onclick={() => { tab = t.key; }}
        class={tab === t.key ? 'pill-on' : 'pill-off'}
      >
        {t.label}
        <span style="margin-left: 6px; font-size: 10px;">({t.count()})</span>
      </button>
    {/each}
  </div>

  <!-- Loading -->
  {#if loading}
    <div class="space-y-3">
      {#each Array(3) as _}
        <div class="card-term" style="padding: 20px; animation: pulse 1.5s ease-in-out infinite;">
          <div style="height: 14px; background: var(--night-edge); border-radius: 4px; width: 33%; margin-bottom: 12px;"></div>
          <div style="height: 11px; background: var(--night-edge); border-radius: 4px; width: 50%;"></div>
        </div>
      {/each}
    </div>

  <!-- Error -->
  {:else if error}
    <div style="background: rgba(239,68,68,0.1); border: 1px solid rgba(239,68,68,0.3); border-radius: var(--r-lg); padding: 24px; text-align: center;">
      <p style="color: #f87171; font-family: var(--mono-product); font-size: 13px;">{error}</p>
      <button
        onclick={loadBots}
        style="margin-top: 12px; padding: 6px 16px; font-size: 11px; font-family: var(--mono-product); color: #f87171; border: 1px solid rgba(239,68,68,0.3); border-radius: 6px; background: transparent; cursor: pointer; transition: background var(--dur-fast) var(--ease-standard);"
      >
        Retry
      </button>
    </div>

  <!-- Active Tab -->
  {:else if tab === 'active'}
    {#if active.length === 0}
      <div class="card-term" style="padding: 32px; text-align: center; color: var(--glow-mute); font-family: var(--sans-product); font-size: 14px;">
        No active bots.
      </div>
    {:else}
      <div style="overflow-x: auto;">
        <table style="width: 100%; border-collapse: collapse; font-family: var(--sans-product); font-size: 14px;">
          <thead>
            <tr style="border-bottom: 1px solid var(--night-rule2);">
              <th class="mono-label" style="text-align: left; padding: 12px 16px;">Name</th>
              <th class="mono-label" style="text-align: left; padding: 12px 16px;">Kind</th>
              <th class="mono-label" style="text-align: left; padding: 12px 16px;">Endpoint</th>
              <th class="mono-label" style="text-align: left; padding: 12px 16px;">Model</th>
              <th class="mono-label" style="text-align: left; padding: 12px 16px;">Added</th>
              <th class="mono-label" style="text-align: right; padding: 12px 16px;">Action</th>
            </tr>
          </thead>
          <tbody>
            {#each active as bot (bot.id)}
              <tr class="bot-row" style="border-bottom: 1px solid var(--night-rule2);">
                <td style="padding: 12px 16px; font-weight: 600; font-size: 15px; color: var(--glow-txt);">{bot.name}</td>
                <td style="padding: 12px 16px;">
                  {#if bot.bot_kind === 'text_only'}
                    <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">text-only</span>
                  {:else}
                    <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">external</span>
                  {/if}
                </td>
                <td style="padding: 12px 16px; max-width: 192px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                  <span class="mono-label" style="font-size: 11px;">{bot.endpoint_url}</span>
                </td>
                <td style="padding: 12px 16px;">
                  {#if bot.model_family}
                    <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">{bot.model_family}</span>
                  {:else}
                    <span style="color: var(--glow-faint);">&mdash;</span>
                  {/if}
                </td>
                <td style="padding: 12px 16px; font-size: 12px; color: var(--glow-mute);">{formatDate(bot.created_at)}</td>
                <td style="padding: 12px 16px; text-align: right;">
                  <button
                    onclick={() => handleAction('deactivate', bot.id)}
                    disabled={actionLoading === bot.id}
                    class="btn-dark-ghost"
                    style="font-size: 12px; padding: 4px 12px;"
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
      <div class="card-term" style="padding: 32px; text-align: center; color: var(--glow-mute); font-family: var(--sans-product); font-size: 14px;">
        No pending submissions.
      </div>
    {:else}
      <div class="space-y-4">
        {#each pending as bot (bot.id)}
          <div class="card-term card-term-hover" style="padding: 20px;">
            <div style="display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; flex-wrap: wrap;">
              <div style="flex: 1; min-width: 0;">
                <div style="display: flex; align-items: center; gap: 10px; margin-bottom: 8px; flex-wrap: wrap;">
                  <h3 style="font-family: var(--sans-product); font-weight: 600; font-size: 15px; color: var(--glow-txt); margin: 0;">{bot.name}</h3>
                  <StatusBadge status={bot.status} />
                  {#if bot.bot_kind === 'text_only'}
                    <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">text-only</span>
                  {:else}
                    <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">external</span>
                  {/if}
                  {#if bot.model_family}
                    <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">{bot.model_family}</span>
                  {/if}
                </div>

                {#if bot.introduction}
                  <div style="margin-bottom: 12px; background: rgba(99,102,241,0.08); border: 1px solid rgba(99,102,241,0.2); border-radius: 8px; padding: 12px;">
                    <div class="mono-label" style="color: var(--indigo-400); margin-bottom: 6px;">
                      Introduction &mdash; primary signal
                    </div>
                    <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-dim); line-height: 1.6; font-style: italic;">&ldquo;{bot.introduction}&rdquo;</p>
                  </div>
                {/if}

                <div style="display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--glow-mute);">
                  <p style="word-break: break-all;">
                    <span class="mono-label" style="display: inline;">Endpoint:</span>
                    <span style="color: var(--glow-dim); margin-left: 4px;">{bot.endpoint_url}</span>
                  </p>
                  {#if bot.description}
                    <p>
                      <span class="mono-label" style="display: inline;">Description:</span>
                      <span style="color: var(--glow-dim); margin-left: 4px;">{bot.description}</span>
                    </p>
                  {/if}
                  {#if bot.submitted_by}
                    <p>
                      <span class="mono-label" style="display: inline;">Submitted by:</span>
                      <span style="color: var(--glow-dim); margin-left: 4px;">{bot.submitted_by}</span>
                    </p>
                  {/if}
                  <p>
                    <span class="mono-label" style="display: inline;">Date:</span>
                    <span style="color: var(--glow-dim); margin-left: 4px;">{formatDate(bot.created_at)}</span>
                  </p>
                </div>
                {#if bot.status === 'smoke_test_failed' && bot.rejection_reason}
                  <div style="margin-top: 12px; background: rgba(245,158,11,0.1); border: 1px solid rgba(245,158,11,0.3); border-radius: 8px; padding: 12px;">
                    <div class="mono-label" style="color: #fbbf24; margin-bottom: 4px;">
                      Smoke test failed
                    </div>
                    <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-dim);">{bot.rejection_reason}</p>
                  </div>
                {/if}
              </div>
              <div style="display: flex; gap: 8px; flex-shrink: 0;">
                <button
                  onclick={() => handleAction('approve', bot.id)}
                  disabled={actionLoading === bot.id}
                  class="btn-indigo"
                  style="font-size: 12px; padding: 6px 14px;"
                >
                  {actionLoading === bot.id
                    ? '...'
                    : (bot.status === 'smoke_test_failed' ? 'Retry approval' : 'Approve')}
                </button>
                <button
                  onclick={() => { rejectingBot = bot; rejectReason = ''; }}
                  disabled={actionLoading === bot.id}
                  class="btn-dark-ghost"
                  style="font-size: 12px; padding: 6px 14px; color: #f87171; border-color: rgba(239,68,68,0.3);"
                >
                  Reject
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
      <div class="card-term" style="padding: 32px; text-align: center; color: var(--glow-mute); font-family: var(--sans-product); font-size: 14px;">
        No inactive bots.
      </div>
    {:else}
      <div style="overflow-x: auto;">
        <table style="width: 100%; border-collapse: collapse; font-family: var(--sans-product); font-size: 14px;">
          <thead>
            <tr style="border-bottom: 1px solid var(--night-rule2);">
              <th class="mono-label" style="text-align: left; padding: 12px 16px;">Name</th>
              <th class="mono-label" style="text-align: left; padding: 12px 16px;">Kind</th>
              <th class="mono-label" style="text-align: left; padding: 12px 16px;">Endpoint</th>
              <th class="mono-label" style="text-align: left; padding: 12px 16px;">Model</th>
              <th class="mono-label" style="text-align: left; padding: 12px 16px;">Status</th>
              <th class="mono-label" style="text-align: right; padding: 12px 16px;">Action</th>
            </tr>
          </thead>
          <tbody>
            {#each inactive as bot (bot.id)}
              <tr class="bot-row" style="border-bottom: 1px solid var(--night-rule2);">
                <td style="padding: 12px 16px; font-weight: 600; font-size: 15px; color: var(--glow-txt);">{bot.name}</td>
                <td style="padding: 12px 16px;">
                  {#if bot.bot_kind === 'text_only'}
                    <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">text-only</span>
                  {:else}
                    <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">external</span>
                  {/if}
                </td>
                <td style="padding: 12px 16px; max-width: 192px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;">
                  <span class="mono-label" style="font-size: 11px;">{bot.endpoint_url}</span>
                </td>
                <td style="padding: 12px 16px;">
                  {#if bot.model_family}
                    <span class="pill-off" style="font-size: 10px; padding: 2px 8px;">{bot.model_family}</span>
                  {:else}
                    <span style="color: var(--glow-faint);">&mdash;</span>
                  {/if}
                </td>
                <td style="padding: 12px 16px;"><StatusBadge status={bot.status} /></td>
                <td style="padding: 12px 16px; text-align: right;">
                  <button
                    onclick={() => handleAction('reactivate', bot.id)}
                    disabled={actionLoading === bot.id}
                    class="btn-indigo"
                    style="font-size: 12px; padding: 4px 12px;"
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

  <!-- Reject modal -->
  {#if rejectingBot}
    <div
      style="position: fixed; inset: 0; background: rgba(0,0,0,0.7); display: flex; align-items: center; justify-content: center; z-index: 50;"
      role="dialog"
      aria-modal="true"
      aria-labelledby="reject-title"
    >
      <div class="card-term" style="padding: 24px; width: 100%; max-width: 440px; margin: 0 16px;">
        <h3 id="reject-title" style="font-family: var(--mono-product); font-size: 13px; color: var(--glow-txt); margin-bottom: 12px;">
          Reject {rejectingBot.name}
        </h3>
        <p style="font-family: var(--sans-product); font-size: 12px; color: var(--glow-mute); margin-bottom: 12px;">
          Enter a reason (min 10 chars, max 500). This is shown to the submitter.
        </p>
        <textarea
          bind:value={rejectReason}
          rows={4}
          maxlength={500}
          placeholder="Reason for rejection..."
          style="width: 100%; padding: 10px 14px; background: var(--night); border: 1px solid var(--night-rule2); border-radius: 8px; font-family: var(--sans-product); font-size: 13px; color: var(--glow-txt); resize: none; box-sizing: border-box;"
        ></textarea>
        <p style="font-family: var(--sans-product); font-size: 11px; color: var(--glow-mute); margin-top: 4px; text-align: right;">
          {rejectReason.trim().length} / 500
          {#if rejectReason.trim().length > 0 && rejectReason.trim().length < 10}
            <span style="color: #fbbf24;">(min 10)</span>
          {/if}
        </p>
        <div style="margin-top: 12px; display: flex; justify-content: flex-end; gap: 8px;">
          <button
            onclick={() => { rejectingBot = null; rejectReason = ''; }}
            class="btn-dark-ghost"
          >
            Cancel
          </button>
          <button
            disabled={rejectReason.trim().length < 10 || submittingReject}
            onclick={confirmReject}
            style="padding: 8px 16px; font-size: 13px; border-radius: 8px; background: #dc2626; color: white; border: none; cursor: pointer; transition: background var(--dur-fast) var(--ease-standard);"
          >
            {submittingReject ? 'Rejecting...' : 'Reject'}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .bot-row:last-child {
    border-bottom: none;
  }
  .bot-row:hover td {
    background: rgba(255, 255, 255, 0.02);
  }
</style>
