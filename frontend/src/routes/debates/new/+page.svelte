<script lang="ts">
  import { api, ApiError } from '$lib/api/client';
  import type { BotResponse } from '$lib/types';
  import { goto } from '$app/navigation';
  import { me } from '$lib/stores/me';

  // Admin-only guard: non-admins are redirected back to the debate list.
  $effect(() => {
    if ($me && $me.role !== 'admin') {
      goto('/debates');
    }
  });

  let topic = $state('');
  let goalMode = $state('adversarial');
  let selectedBotIds = $state<Set<string>>(new Set());
  let showAdvanced = $state(false);

  let bots = $state<BotResponse[]>([]);
  let loading = $state(true);
  let loadError = $state<string | null>(null);
  let submitting = $state(false);
  let submitError = $state<string | null>(null);

  let activeBots = $derived(bots.filter(b => b.status === 'active'));
  let selectionCount = $derived(selectedBotIds.size);
  let canSubmit = $derived(
    topic.trim().length > 0 && selectionCount >= 3 && selectionCount <= 5 && !submitting,
  );

  const GOAL_MODES = [
    { value: 'adversarial', label: 'Adversarial', enabled: true, note: 'default' },
    { value: 'consensus', label: 'Consensus-seeking', enabled: false, note: 'Coming soon' },
    { value: 'winner', label: 'Winner-takes-all', enabled: false, note: 'Coming soon' },
    { value: 'devils_advocate', label: "Devil's Advocate Stress Test", enabled: false, note: 'Coming soon' },
  ] as const;

  const ROLES = [
    { name: 'Proponent', description: 'Argues the strongest case in favour of the proposition.' },
    { name: 'Skeptic', description: 'Challenges assumptions, demands evidence, flags weaknesses.' },
    { name: "Devil's Advocate", description: 'Argues the opposite position regardless of personal stance.' },
    { name: 'Empiricist', description: 'Grounds the debate in data, precedent, and verifiable claims.' },
    { name: 'Steelman', description: 'Strengthens the weakest argument before it can be dismissed.' },
  ] as const;

  function toggleBot(id: string) {
    const next = new Set(selectedBotIds);
    if (next.has(id)) {
      next.delete(id);
    } else if (next.size < 5) {
      next.add(id);
    }
    selectedBotIds = next;
  }

  function selectAll() {
    selectedBotIds = new Set(activeBots.slice(0, 5).map(b => b.id));
  }

  function clearSelection() {
    selectedBotIds = new Set();
  }

  async function handleSubmit() {
    if (!canSubmit) return;
    submitting = true;
    submitError = null;
    try {
      const debate = await api.debates.create({
        topic: topic.trim(),
        bot_ids: [...selectedBotIds],
        goal_mode: goalMode,
      });
      goto(`/debates/${debate.id}`);
    } catch (e) {
      if (e instanceof ApiError) {
        submitError = `API error ${e.status}: ${JSON.stringify(e.body)}`;
      } else {
        submitError = e instanceof Error ? e.message : 'Failed to create debate';
      }
    } finally {
      submitting = false;
    }
  }

  $effect(() => {
    api.bots
      .list()
      .then(data => {
        bots = data;
      })
      .catch(e => {
        loadError = e.message ?? 'Failed to load bots';
      })
      .finally(() => {
        loading = false;
      });
  });
</script>

<div style="max-width: 768px;">
  <!-- Page header -->
  <p class="tm-eyebrow mb-2" style="color: var(--indigo-400);">Workspace · Create</p>
  <h1 style="font-family: var(--sans-product); font-weight: 800; font-size: 32px; letter-spacing: -0.02em; color: var(--glow-txt); margin-bottom: 24px;">New Debate</h1>

  <!-- Back link -->
  <div style="margin-bottom: 32px;">
    <a
      href="/debates"
      class="btn-dark-ghost no-underline"
    >
      &larr; Cancel
    </a>
  </div>

  <!-- Topic -->
  <div style="margin-bottom: 24px;">
    <label for="topic" class="mono-label" style="display: block; margin-bottom: 6px; color: var(--indigo-400);">
      Topic
    </label>
    <input
      id="topic"
      type="text"
      bind:value={topic}
      placeholder="Enter the debate topic or question..."
      style="background: var(--night-raise); border: 1px solid var(--night-rule2); border-radius: 8px; padding: 10px 14px; font-family: var(--sans-product); font-size: 14px; color: var(--glow-txt); width: 100%; box-sizing: border-box; transition: border-color var(--dur-fast) var(--ease-standard); outline: none;"
    />
  </div>

  <!-- Bot Selection -->
  <div style="margin-bottom: 24px;">
    <div style="display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px;">
      <span class="mono-label" style="color: var(--indigo-400);">
        Select Bots
        <span style="color: var(--glow-mute); font-weight: normal;">(3&ndash;5 required)</span>
      </span>
      <div style="display: flex; gap: 8px; align-items: center;">
        <button
          onclick={selectAll}
          style="font-family: var(--mono-product); font-size: 11px; letter-spacing: 0.1em; color: var(--indigo-400); background: none; border: none; cursor: pointer; padding: 0; transition: color var(--dur-fast) var(--ease-standard);"
        >
          Select all
        </button>
        <span style="color: var(--glow-faint);">/</span>
        <button
          onclick={clearSelection}
          style="font-family: var(--mono-product); font-size: 11px; letter-spacing: 0.1em; color: var(--glow-mute); background: none; border: none; cursor: pointer; padding: 0; transition: color var(--dur-fast) var(--ease-standard);"
        >
          Clear
        </button>
      </div>
    </div>

    {#if loading}
      <div class="card-term" style="padding: 24px;">
        <div style="display: flex; flex-direction: column; gap: 12px;">
          {#each Array(3) as _}
            <div style="height: 40px; background: var(--night-rule2); border-radius: 6px; animation: pulse 1.5s ease-in-out infinite;"></div>
          {/each}
        </div>
      </div>
    {:else if loadError}
      <div style="background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: 12px; padding: 16px; text-align: center;">
        <p style="font-family: var(--mono-product); font-size: 13px; color: #EF4444; margin: 0;">{loadError}</p>
      </div>
    {:else if activeBots.length === 0}
      <div
        class="card-term"
        style="padding: 24px; text-align: center;"
      >
        <p style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-mute); margin: 0;">No active bots available. Register bots first.</p>
      </div>
    {:else}
      <div style="display: flex; flex-direction: column; gap: 8px;">
        {#each activeBots as bot (bot.id)}
          <label
            class="card-term {selectedBotIds.has(bot.id) ? '' : 'card-term-hover'}"
            style="{selectedBotIds.has(bot.id) ? 'border-color: var(--indigo-500); background: rgba(99,102,241,0.05);' : ''} cursor: pointer; display: flex; align-items: center; gap: 12px; padding: 14px;"
          >
            <input
              type="checkbox"
              checked={selectedBotIds.has(bot.id)}
              onchange={() => toggleBot(bot.id)}
              disabled={!selectedBotIds.has(bot.id) && selectionCount >= 5}
              style="accent-color: var(--indigo-500); flex-shrink: 0;"
            />
            <span style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-txt);">{bot.name}</span>
            {#if bot.model_family}
              <span style="font-family: var(--mono-product); font-size: 10px; letter-spacing: 0.1em; color: var(--glow-mute); padding: 2px 6px; background: var(--night-edge); border-radius: 4px; margin-left: auto;">
                {bot.model_family}
              </span>
            {/if}
          </label>
        {/each}
      </div>
      <p style="font-family: var(--sans-product); font-size: 12px; color: var(--glow-mute); margin-top: 6px;">
        {selectionCount} of {activeBots.length} selected
        {#if selectionCount < 3}
          <span style="color: #F59E0B;"> &mdash; select at least 3</span>
        {/if}
      </p>
    {/if}
  </div>

  <!-- Goal Mode -->
  <div style="margin-bottom: 24px;">
    <span class="mono-label" style="display: block; margin-bottom: 8px; color: var(--indigo-400);">Goal Mode</span>
    <div style="display: flex; flex-direction: column; gap: 8px;">
      {#each GOAL_MODES as mode}
        <label
          class="{mode.enabled ? 'card-term-hover' : ''}"
          style="display: flex; align-items: center; gap: 12px; padding: 12px 16px; background: var(--night-raise); border: 1px solid {goalMode === mode.value ? 'var(--indigo-500)' : 'var(--night-rule2)'}; border-radius: 8px; {goalMode === mode.value ? 'background: rgba(99,102,241,0.05);' : ''} {mode.enabled ? 'cursor: pointer;' : 'opacity: 0.45; cursor: not-allowed;'} transition: border-color var(--dur-fast) var(--ease-standard);"
        >
          <input
            type="radio"
            name="goalMode"
            value={mode.value}
            checked={goalMode === mode.value}
            onchange={() => { goalMode = mode.value; }}
            disabled={!mode.enabled}
            style="accent-color: var(--indigo-500); flex-shrink: 0;"
          />
          <span style="font-family: var(--sans-product); font-size: 14px; color: {mode.enabled ? 'var(--glow-txt)' : 'var(--glow-mute)'};">
            {mode.label}
          </span>
          <span style="font-family: var(--mono-product); font-size: 10px; letter-spacing: 0.1em; color: var(--glow-mute); margin-left: auto;">
            {mode.note}
          </span>
        </label>
      {/each}
    </div>
  </div>

  <!-- Advanced (collapsed) -->
  <div style="margin-bottom: 32px;">
    <button
      onclick={() => { showAdvanced = !showAdvanced; }}
      style="display: flex; align-items: center; gap: 8px; font-family: var(--sans-product); font-size: 14px; color: var(--glow-dim); background: none; border: none; cursor: pointer; padding: 0; transition: color var(--dur-fast) var(--ease-standard);"
    >
      <span style="font-family: var(--mono-product); font-size: 11px;">{showAdvanced ? '\u25BC' : '\u25B6'}</span>
      Advanced protocol details
    </button>

    {#if showAdvanced}
      <div class="card-term" style="margin-top: 12px; padding: 20px; display: flex; flex-direction: column; gap: 16px;">
        <div>
          <span class="mono-label" style="display: block; margin-bottom: 4px;">Rounds</span>
          <p style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-txt); margin: 0;">5 (fixed)</p>
        </div>
        <div>
          <span class="mono-label" style="display: block; margin-bottom: 8px;">Constitutional Roles</span>
          <div style="overflow-x: auto;">
            <table style="width: 100%; border-collapse: collapse; font-size: 13px;">
              <thead>
                <tr style="border-bottom: 1px solid var(--night-rule2);">
                  <th style="text-align: left; padding: 8px 16px 8px 0; font-family: var(--mono-product); font-size: 10px; letter-spacing: 0.2em; text-transform: uppercase; color: var(--glow-mute); font-weight: normal;">
                    Role
                  </th>
                  <th style="text-align: left; padding: 8px 0; font-family: var(--mono-product); font-size: 10px; letter-spacing: 0.2em; text-transform: uppercase; color: var(--glow-mute); font-weight: normal;">
                    Description
                  </th>
                </tr>
              </thead>
              <tbody>
                {#each ROLES as role}
                  <tr style="border-bottom: 1px solid var(--night-rule2);">
                    <td style="padding: 8px 16px 8px 0; font-family: var(--mono-product); font-size: 11px; color: var(--indigo-400);">{role.name}</td>
                    <td style="padding: 8px 0; font-family: var(--sans-product); font-size: 12px; color: var(--glow-dim);">{role.description}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    {/if}
  </div>

  <!-- Submit Error -->
  {#if submitError}
    <div style="background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: 12px; padding: 16px; margin-bottom: 16px;">
      <p style="font-family: var(--sans-product); font-size: 12px; color: #EF4444; margin: 0;">Error · {submitError}</p>
    </div>
  {/if}

  <!-- Launch Button -->
  <div style="display: flex; gap: 12px; align-items: center;">
    {#if canSubmit}
      <button
        onclick={handleSubmit}
        disabled={!canSubmit}
        class="btn-indigo"
        style="padding: 12px 28px;"
      >
        {#if submitting}
          <span style="display: inline-flex; align-items: center; gap: 8px;">
            <span style="width: 16px; height: 16px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.75s linear infinite; display: inline-block;"></span>
            Launching...
          </span>
        {:else}
          Create Debate &rarr;
        {/if}
      </button>
    {:else}
      <button
        onclick={handleSubmit}
        disabled
        style="padding: 12px 28px; background: var(--night-edge); border: 1px solid var(--night-rule2); border-radius: 8px; font-family: var(--sans-product); font-size: 14px; font-weight: 500; color: var(--glow-mute); cursor: not-allowed;"
      >
        {#if submitting}
          <span style="display: inline-flex; align-items: center; gap: 8px;">
            <span style="width: 16px; height: 16px; border: 2px solid rgba(255,255,255,0.15); border-top-color: rgba(255,255,255,0.4); border-radius: 50%; animation: spin 0.75s linear infinite; display: inline-block;"></span>
            Launching...
          </span>
        {:else}
          Create Debate &rarr;
        {/if}
      </button>
    {/if}
    <a href="/debates" class="btn-dark-ghost no-underline">← Cancel</a>
  </div>
</div>

<style>
  input:focus {
    border-color: var(--gold) !important;
    box-shadow: 0 0 0 3px rgba(196, 160, 82, 0.18);
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
</style>
