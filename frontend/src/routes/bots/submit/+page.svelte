<script lang="ts">
  import { api, ApiError } from '$lib/api/client';
  import { goto } from '$app/navigation';

  let name = $state('');
  let endpointUrl = $state('');
  let token = $state('');
  let modelFamily = $state('');
  let description = $state('');
  let submitting = $state(false);
  let error = $state<string | null>(null);

  const MODEL_FAMILIES = [
    { value: '', label: 'Select model family...' },
    { value: 'claude', label: 'Claude' },
    { value: 'gpt4', label: 'GPT-4' },
    { value: 'llama', label: 'LLaMA' },
    { value: 'minimax', label: 'MiniMax' },
    { value: 'gemini', label: 'Gemini' },
    { value: 'other', label: 'Other' },
  ] as const;

  let charCount = $derived(description.length);
  let canSubmit = $derived(
    name.trim().length > 0 &&
    endpointUrl.trim().length > 0 &&
    token.trim().length > 0 &&
    !submitting,
  );

  async function handleSubmit() {
    if (!canSubmit) return;
    submitting = true;
    error = null;
    try {
      await api.bots.create({
        name: name.trim(),
        endpoint_url: endpointUrl.trim(),
        token: token.trim(),
        model_family: modelFamily || undefined,
        description: description.trim() || undefined,
      });
      goto('/bots/my-submissions');
    } catch (e) {
      if (e instanceof ApiError) {
        error = `API error ${e.status}: ${JSON.stringify(e.body)}`;
      } else {
        error = e instanceof Error ? e.message : 'Failed to submit bot';
      }
    } finally {
      submitting = false;
    }
  }
</script>

<div class="max-w-2xl">
  <div class="mb-8">
    <a
      href="/bots"
      class="text-xs mono text-[var(--text-muted)] hover:text-[var(--text-secondary)] transition-colors no-underline"
    >
      &larr; Back to bots
    </a>
    <h1 class="mono text-2xl font-bold mt-2">Submit a Bot</h1>
    <p class="text-sm text-[var(--text-muted)] mt-1">
      Submit a bot for review. Once approved it can participate in debates.
      <a href="/bots/criteria" class="text-[#8b5cf6] hover:text-[#a78bfa] no-underline">
        What are the approval criteria?
      </a>
    </p>
  </div>

  <!-- Name -->
  <div class="mb-5">
    <label for="bot-name" class="block text-sm font-medium text-[var(--text-secondary)] mb-1.5">
      Bot Name <span class="text-red-400">*</span>
    </label>
    <input
      id="bot-name"
      type="text"
      bind:value={name}
      placeholder="e.g. Aristotle-Claude"
      class="w-full px-4 py-2.5 bg-[var(--surface)] border border-[var(--border)] rounded-lg text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[#8b5cf6]/50 transition-colors"
    />
  </div>

  <!-- Endpoint URL -->
  <div class="mb-5">
    <label for="endpoint" class="block text-sm font-medium text-[var(--text-secondary)] mb-1.5">
      Endpoint URL <span class="text-red-400">*</span>
    </label>
    <input
      id="endpoint"
      type="text"
      bind:value={endpointUrl}
      placeholder="https://your-bot.example.com/debate"
      class="w-full px-4 py-2.5 bg-[var(--surface)] border border-[var(--border)] rounded-lg text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[#8b5cf6]/50 transition-colors mono"
    />
  </div>

  <!-- Bearer Token -->
  <div class="mb-5">
    <label for="token" class="block text-sm font-medium text-[var(--text-secondary)] mb-1.5">
      Bearer Token <span class="text-red-400">*</span>
    </label>
    <input
      id="token"
      type="password"
      bind:value={token}
      placeholder="Your bot's authentication token"
      class="w-full px-4 py-2.5 bg-[var(--surface)] border border-[var(--border)] rounded-lg text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[#8b5cf6]/50 transition-colors mono"
    />
  </div>

  <!-- Model Family -->
  <div class="mb-5">
    <label for="model-family" class="block text-sm font-medium text-[var(--text-secondary)] mb-1.5">
      Model Family
    </label>
    <select
      id="model-family"
      bind:value={modelFamily}
      class="w-full px-4 py-2.5 bg-[var(--surface)] border border-[var(--border)] rounded-lg text-sm text-[var(--text-primary)] focus:outline-none focus:border-[#8b5cf6]/50 transition-colors"
    >
      {#each MODEL_FAMILIES as mf}
        <option value={mf.value}>{mf.label}</option>
      {/each}
    </select>
  </div>

  <!-- Description -->
  <div class="mb-6">
    <label for="description" class="block text-sm font-medium text-[var(--text-secondary)] mb-1.5">
      Description
    </label>
    <textarea
      id="description"
      bind:value={description}
      maxlength={500}
      rows={4}
      placeholder="Describe what makes this bot interesting for debates..."
      class="w-full px-4 py-2.5 bg-[var(--surface)] border border-[var(--border)] rounded-lg text-sm text-[var(--text-primary)] placeholder:text-[var(--text-muted)] focus:outline-none focus:border-[#8b5cf6]/50 transition-colors resize-none"
    ></textarea>
    <p class="text-xs text-[var(--text-muted)] mt-1 text-right">
      <span class={charCount > 450 ? 'text-amber-400' : ''}>{charCount}</span> / 500
    </p>
  </div>

  <!-- Error -->
  {#if error}
    <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-4 mb-4">
      <p class="text-red-400 mono text-sm">{error}</p>
    </div>
  {/if}

  <!-- Submit -->
  <button
    onclick={handleSubmit}
    disabled={!canSubmit}
    class="w-full py-3 rounded-lg text-sm font-medium transition-colors {canSubmit ? 'bg-[#8b5cf6] text-white hover:bg-[#7c3aed] cursor-pointer' : 'bg-[var(--border)] text-[var(--text-muted)] cursor-not-allowed'}"
  >
    {#if submitting}
      <span class="inline-flex items-center gap-2">
        <span class="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin"></span>
        Submitting...
      </span>
    {:else}
      Submit Bot for Review
    {/if}
  </button>
</div>
