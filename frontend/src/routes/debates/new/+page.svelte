<script lang="ts">
  import { api, ApiError } from '$lib/api/client';
  import type { BotResponse } from '$lib/types';
  import { goto } from '$app/navigation';

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

<div class="max-w-3xl">
  <div class="mb-8">
    <a
      href="/debates"
      class="text-xs mono text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors no-underline"
    >
      &larr; Back to debates
    </a>
    <h1 class="mono text-2xl font-bold mt-2">New Debate</h1>
  </div>

  <!-- Topic -->
  <div class="mb-6">
    <label for="topic" class="block text-sm font-medium text-[var(--text-secondary)] mb-2">
      Topic
    </label>
    <input
      id="topic"
      type="text"
      bind:value={topic}
      placeholder="Enter the debate topic or question..."
      class="w-full px-4 py-3 bg-[var(--surface)] border border-[var(--border)] rounded-lg text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[#8b5cf6]/50 transition-colors"
    />
  </div>

  <!-- Bot Selection -->
  <div class="mb-6">
    <div class="flex items-center justify-between mb-2">
      <span class="text-sm font-medium text-[var(--text-secondary)]">
        Select Bots
        <span class="text-[var(--text-muted)] font-normal">(3&ndash;5 required)</span>
      </span>
      <div class="flex gap-2">
        <button
          onclick={selectAll}
          class="text-xs mono text-[#8b5cf6] hover:text-[#a78bfa] transition-colors"
        >
          Select all
        </button>
        <span class="text-[var(--text-muted)]">/</span>
        <button
          onclick={clearSelection}
          class="text-xs mono text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors"
        >
          Clear
        </button>
      </div>
    </div>

    {#if loading}
      <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-6">
        <div class="space-y-3">
          {#each Array(3) as _}
            <div class="h-10 bg-[var(--border)] rounded animate-pulse"></div>
          {/each}
        </div>
      </div>
    {:else if loadError}
      <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-4 text-center">
        <p class="text-red-400 mono text-sm">{loadError}</p>
      </div>
    {:else if activeBots.length === 0}
      <div
        class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-6 text-center text-[var(--text-muted)] text-sm"
      >
        No active bots available. Register bots first.
      </div>
    {:else}
      <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg divide-y divide-[var(--border)]">
        {#each activeBots as bot (bot.id)}
          <label
            class="flex items-center gap-3 px-4 py-3 cursor-pointer hover:bg-[rgba(139,92,246,0.05)] transition-colors {selectedBotIds.has(bot.id) ? 'bg-[rgba(139,92,246,0.08)]' : ''}"
          >
            <input
              type="checkbox"
              checked={selectedBotIds.has(bot.id)}
              onchange={() => toggleBot(bot.id)}
              disabled={!selectedBotIds.has(bot.id) && selectionCount >= 5}
              class="accent-[#8b5cf6]"
            />
            <span class="text-sm text-[var(--text-primary)]">{bot.name}</span>
            {#if bot.model_family}
              <span class="text-[10px] mono text-[var(--text-muted)] px-1.5 py-0.5 bg-[var(--border)] rounded">
                {bot.model_family}
              </span>
            {/if}
          </label>
        {/each}
      </div>
      <p class="text-xs text-[var(--text-muted)] mt-1.5">
        {selectionCount} of {activeBots.length} selected
        {#if selectionCount < 3}
          <span class="text-amber-400"> &mdash; select at least 3</span>
        {/if}
      </p>
    {/if}
  </div>

  <!-- Goal Mode -->
  <div class="mb-6">
    <span class="block text-sm font-medium text-[var(--text-secondary)] mb-2">Goal Mode</span>
    <div class="space-y-2">
      {#each GOAL_MODES as mode}
        <label
          class="flex items-center gap-3 px-4 py-2.5 bg-[var(--surface)] border border-[var(--border)] rounded-lg {mode.enabled ? 'cursor-pointer hover:border-[#8b5cf6]/30' : 'opacity-50 cursor-not-allowed'} transition-colors {goalMode === mode.value ? 'border-[#8b5cf6]/50 bg-[rgba(139,92,246,0.08)]' : ''}"
        >
          <input
            type="radio"
            name="goalMode"
            value={mode.value}
            checked={goalMode === mode.value}
            onchange={() => { goalMode = mode.value; }}
            disabled={!mode.enabled}
            class="accent-[#8b5cf6]"
          />
          <span class="text-sm {mode.enabled ? 'text-[var(--text-primary)]' : 'text-[var(--text-muted)]'}">
            {mode.label}
          </span>
          <span class="text-[10px] mono text-[var(--text-muted)] ml-auto">
            {mode.note}
          </span>
        </label>
      {/each}
    </div>
  </div>

  <!-- Advanced (collapsed) -->
  <div class="mb-8">
    <button
      onclick={() => { showAdvanced = !showAdvanced; }}
      class="flex items-center gap-2 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
    >
      <span class="mono text-xs">{showAdvanced ? '\u25BC' : '\u25B6'}</span>
      Advanced protocol details
    </button>

    {#if showAdvanced}
      <div class="mt-3 bg-[var(--surface)] border border-[var(--border)] rounded-lg p-5 space-y-4">
        <div>
          <span class="text-xs mono text-[var(--text-muted)]">Rounds</span>
          <p class="text-sm text-[var(--text-primary)]">5 (fixed)</p>
        </div>
        <div>
          <span class="text-xs mono text-[var(--text-muted)] mb-2 block">Constitutional Roles</span>
          <div class="overflow-x-auto">
            <table class="w-full text-sm">
              <thead>
                <tr class="border-b border-[var(--border)]">
                  <th class="text-left py-2 pr-4 text-xs mono text-[var(--text-muted)] font-normal">
                    Role
                  </th>
                  <th class="text-left py-2 text-xs mono text-[var(--text-muted)] font-normal">
                    Description
                  </th>
                </tr>
              </thead>
              <tbody>
                {#each ROLES as role}
                  <tr class="border-b border-[var(--border)] last:border-0">
                    <td class="py-2 pr-4 mono text-xs text-[#8b5cf6]">{role.name}</td>
                    <td class="py-2 text-[var(--text-secondary)] text-xs">{role.description}</td>
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
    <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-4 mb-4">
      <p class="text-red-400 mono text-sm">{submitError}</p>
    </div>
  {/if}

  <!-- Launch Button -->
  <button
    onclick={handleSubmit}
    disabled={!canSubmit}
    class="w-full py-3 rounded-lg text-sm font-medium transition-colors {canSubmit ? 'bg-[#8b5cf6] text-white hover:bg-[#7c3aed] cursor-pointer' : 'bg-[var(--border)] text-[var(--text-muted)] cursor-not-allowed'}"
  >
    {#if submitting}
      <span class="inline-flex items-center gap-2">
        <span class="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></span>
        Launching...
      </span>
    {:else}
      Launch Debate
    {/if}
  </button>
</div>
