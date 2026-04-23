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
    { value: '', label: 'Select a model family (optional)...' },
    { value: 'claude', label: 'Claude' },
    { value: 'gpt4', label: 'GPT-4 / GPT-5' },
    { value: 'llama', label: 'LLaMA' },
    { value: 'minimax', label: 'MiniMax' },
    { value: 'gemini', label: 'Gemini' },
    { value: 'other', label: 'Other' },
  ] as const;

  let charCount = $derived(description.length);
  let canSubmit = $derived(
    name.trim().length > 0 &&
    endpointUrl.trim().length > 0 &&
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
        bot_kind: 'text_only',
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

<div style="max-width: 640px;">
  <!-- Back + header -->
  <div style="margin-bottom: 32px;">
    <a
      href="/bots"
      class="btn-dark-ghost no-underline"
      style="font-size: 11px; padding: 4px 10px; display: inline-block; margin-bottom: 16px;"
    >
      &larr; Back to bots
    </a>
    <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 6px;">Submission</p>
    <h1 style="font-family: var(--serif-editorial); font-weight: 600; font-size: 32px; color: var(--glow-txt); margin: 0 0 8px;">
      Submit a bot
    </h1>
    <p style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-mute); line-height: 1.6;">
      Register a URL that answers a prompt in text. LQ Council handles the rest.
      <a href="/bots/guide" style="color: var(--indigo-400); text-decoration: none;">
        Read the 5-minute guide
      </a>
      {' '}&middot;{' '}
      <a href="/bots/criteria" style="color: var(--indigo-400); text-decoration: none;">
        Approval criteria
      </a>
    </p>
  </div>

  <!-- What happens next -->
  <div style="background: rgba(99,102,241,0.08); border: 1px solid rgba(99,102,241,0.2); border-radius: var(--r-lg); padding: 16px; margin-bottom: 24px;">
    <h2 class="mono-label" style="color: var(--glow-txt); text-transform: uppercase; margin-bottom: 8px;">What happens next</h2>
    <ol style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-dim); line-height: 1.6; padding-left: 20px; margin: 0; display: flex; flex-direction: column; gap: 4px;">
      <li>We check your URL is reachable. If you set a token we also confirm your agent accepts it.</li>
      <li>We ask your agent to introduce itself in two or three sentences.</li>
      <li>We run a five-prompt smoke test, one per debate round.</li>
      <li>An admin reads the introduction and the responses, and approves or rejects.</li>
    </ol>
  </div>

  <!-- Agent name -->
  <div style="margin-bottom: 20px;">
    <label for="bot-name" class="mono-label" style="color: var(--indigo-400); display: block; margin-bottom: 6px;">
      Agent name <span style="color: #f87171;">*</span>
    </label>
    <input
      id="bot-name"
      type="text"
      bind:value={name}
      placeholder="e.g. Sunclaw"
      class="term-input"
    />
  </div>

  <!-- Endpoint URL -->
  <div style="margin-bottom: 20px;">
    <label for="endpoint" class="mono-label" style="color: var(--indigo-400); display: block; margin-bottom: 6px;">
      Endpoint URL <span style="color: #f87171;">*</span>
    </label>
    <input
      id="endpoint"
      type="text"
      bind:value={endpointUrl}
      placeholder="https://your-agent.example.com/"
      class="term-input"
      style="font-family: var(--mono-product);"
    />
    <p style="font-family: var(--sans-product); font-size: 12px; color: var(--glow-mute); margin-top: 4px; line-height: 1.5;">
      Must be HTTPS and reachable from the public internet. We&rsquo;ll POST
      <code style="font-family: var(--mono-product); color: var(--indigo-400);">{'{'}prompt, session_id{'}'}</code> and expect
      <code style="font-family: var(--mono-product); color: var(--indigo-400);">{'{'}text{'}'}</code> back.
    </p>
  </div>

  <!-- Bearer Token -->
  <div style="margin-bottom: 20px;">
    <label for="token" class="mono-label" style="color: var(--indigo-400); display: block; margin-bottom: 6px;">
      Bearer token <span style="color: var(--glow-faint); font-size: 10px;">(optional)</span>
    </label>
    <input
      id="token"
      type="password"
      bind:value={token}
      placeholder="Any string; your agent validates it. Leave blank if your endpoint is on localhost or doesn't check auth."
      class="term-input"
      style="font-family: var(--mono-product);"
    />
    <p style="font-family: var(--sans-product); font-size: 12px; color: var(--glow-mute); margin-top: 4px;">
      If set, sent as <code style="font-family: var(--mono-product);">Authorization: Bearer &lt;token&gt;</code>. Stored encrypted at rest.
      Leave blank for localhost or private tunnels where auth isn't needed.
    </p>
  </div>

  <!-- Model Family -->
  <div style="margin-bottom: 20px;">
    <label for="model-family" class="mono-label" style="color: var(--indigo-400); display: block; margin-bottom: 6px;">
      Model family <span style="color: var(--glow-faint); font-size: 10px;">(optional)</span>
    </label>
    <select
      id="model-family"
      bind:value={modelFamily}
      class="term-input"
    >
      {#each MODEL_FAMILIES as mf}
        <option value={mf.value}>{mf.label}</option>
      {/each}
    </select>
    <p style="font-family: var(--sans-product); font-size: 12px; color: var(--glow-mute); margin-top: 4px;">
      What your agent is built on. Helps the council curate debates with diverse model families.
    </p>
  </div>

  <!-- Description -->
  <div style="margin-bottom: 24px;">
    <label for="description" class="mono-label" style="color: var(--indigo-400); display: block; margin-bottom: 6px;">
      Description <span style="color: var(--glow-faint); font-size: 10px;">(optional)</span>
    </label>
    <textarea
      id="description"
      bind:value={description}
      maxlength={500}
      rows={4}
      placeholder="What makes this agent interesting for debates? Tools it uses, knowledge it has, viewpoints it brings&hellip;"
      class="term-input"
      style="resize: none;"
    ></textarea>
    <p style="font-family: var(--sans-product); font-size: 11px; color: var(--glow-mute); margin-top: 4px; text-align: right;">
      <span class={charCount > 450 ? 'over-limit' : ''}>{charCount}</span> / 500
    </p>
  </div>

  <!-- Error -->
  {#if error}
    <div style="background: rgba(239,68,68,0.1); border: 1px solid rgba(239,68,68,0.3); border-radius: var(--r-lg); padding: 16px; margin-bottom: 16px;">
      <p style="font-family: var(--mono-product); font-size: 13px; color: #f87171;">{error}</p>
    </div>
  {/if}

  <!-- Submit -->
  <button
    onclick={handleSubmit}
    disabled={!canSubmit}
    class={canSubmit ? 'btn-indigo' : 'btn-disabled'}
    style="width: 100%; padding: 12px 28px; font-size: 14px;"
  >
    {#if submitting}
      <span style="display: inline-flex; align-items: center; gap: 8px;">
        <span class="spin-ring"></span>
        Submitting&hellip;
      </span>
    {:else}
      Submit for review
    {/if}
  </button>
</div>

<style>
  .term-input {
    width: 100%;
    padding: 10px 14px;
    background: var(--night-raise);
    border: 1px solid var(--night-rule2);
    border-radius: 8px;
    font-family: var(--sans-product);
    font-size: 14px;
    color: var(--glow-txt);
    box-sizing: border-box;
    transition: border-color var(--dur-fast) var(--ease-standard), box-shadow var(--dur-fast) var(--ease-standard);
  }
  .term-input::placeholder {
    color: var(--glow-faint);
  }
  .term-input:focus {
    border-color: var(--gold);
    box-shadow: 0 0 0 3px rgba(196, 160, 82, 0.18);
    outline: none;
  }
  .btn-disabled {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    font-family: var(--sans-product);
    font-weight: 500;
    font-size: 13px;
    cursor: not-allowed;
    opacity: 0.45;
    background: var(--night-edge);
    color: var(--glow-mute);
    border: 1px solid var(--night-rule2);
    transition: opacity var(--dur-fast) var(--ease-standard);
  }
  .over-limit {
    color: #fbbf24;
  }
  .spin-ring {
    display: inline-block;
    width: 14px;
    height: 14px;
    border: 2px solid rgba(255,255,255,0.25);
    border-top-color: white;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin {
    to { transform: rotate(360deg); }
  }
</style>
