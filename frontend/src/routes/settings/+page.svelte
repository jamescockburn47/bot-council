<style>
  .prompts-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    font-family: var(--sans-product);
    font-size: 14px;
    color: var(--glow-dim);
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    transition: color var(--dur-fast) var(--ease-standard);
  }
  .prompts-toggle:hover {
    color: var(--glow-txt);
  }
</style>

<script lang="ts">
  let showPrompts = $state(false);

  const ROLES = [
    { name: 'Proponent', description: 'Argues the strongest case in favour of the proposition.', enforcement: 'Must defend the topic position; penalised if neutral or opposing.' },
    { name: 'Skeptic', description: 'Challenges assumptions, demands evidence, flags weaknesses.', enforcement: 'Must issue at least one challenge per round; penalised for agreement without scrutiny.' },
    { name: "Devil's Advocate", description: 'Argues the opposite position regardless of personal stance.', enforcement: 'Must oppose majority position; cannot concede without structural justification.' },
    { name: 'Empiricist', description: 'Grounds the debate in data, precedent, and verifiable claims.', enforcement: 'Must cite evidence or data; penalised for unsupported assertions.' },
    { name: 'Steelman', description: 'Strengthens the weakest argument before it can be dismissed.', enforcement: 'Must improve the weakest opposing argument each round.' },
  ] as const;
</script>

<div style="max-width: 56rem;">
  <!-- Header -->
  <div style="margin-bottom: 2rem;">
    <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 6px;">Workspace · Settings</p>
    <h1 style="font-family: var(--sans-product); font-weight: 800; font-size: 32px; color: var(--glow-txt); margin: 0;">
      Settings
    </h1>
  </div>

  <!-- Banner -->
  <div class="card-term" style="padding: 16px; margin-bottom: 2rem; border-color: rgba(99,102,241,0.3);">
    <p style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-dim); margin: 0;">
      Protocol configuration is read-only in this release. Changes require backend deployment.
    </p>
  </div>

  <!-- Protocol Parameters -->
  <section style="margin-bottom: 2rem;">
    <div class="card-term-lg">
      <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 16px;">Protocol Parameters</p>
      <div style="display: flex; flex-direction: column; gap: 0;">
        {#each [
          { label: 'Rounds', value: '5' },
          { label: 'Quorum', value: '3 bots minimum' },
          { label: 'Response timeout', value: '5 minutes' },
          { label: 'Max retries', value: '2' },
          { label: 'Synthesis model', value: 'claude-opus-4-6' },
          { label: 'Synthesis temperature', value: '0.0' },
        ] as param}
          <div style="display: flex; align-items: center; justify-content: space-between; padding: 12px 0; border-bottom: 1px solid var(--night-rule2);">
            <span style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-dim);">{param.label}</span>
            <span style="font-family: var(--mono-product); font-size: 13px; color: var(--glow-txt); accent-color: var(--indigo-500);">{param.value}</span>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <!-- Constitutional Roles -->
  <section style="margin-bottom: 2rem;">
    <div class="card-term-lg">
      <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 16px;">Constitutional Roles</p>
      <div style="overflow-x: auto;">
        <table style="width: 100%; border-collapse: collapse; font-size: 14px;">
          <thead>
            <tr style="border-bottom: 1px solid var(--night-rule2);">
              <th class="mono-label" style="text-align: left; padding: 10px 0 10px 0; padding-right: 20px;">Role</th>
              <th class="mono-label" style="text-align: left; padding: 10px 20px;">Description</th>
              <th class="mono-label" style="text-align: left; padding: 10px 0 10px 20px;">Enforcement Rule</th>
            </tr>
          </thead>
          <tbody>
            {#each ROLES as role}
              <tr style="border-bottom: 1px solid var(--night-rule2);">
                <td style="padding: 12px 20px 12px 0; font-family: var(--mono-product); font-size: 13px; color: var(--indigo-400); white-space: nowrap;">{role.name}</td>
                <td style="padding: 12px 20px; font-family: var(--sans-product); font-size: 14px; color: var(--glow-dim);">{role.description}</td>
                <td style="padding: 12px 0 12px 20px; font-family: var(--sans-product); font-size: 13px; color: var(--glow-mute);">{role.enforcement}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  </section>

  <!-- Prompt Templates -->
  <section>
    <button
      onclick={() => { showPrompts = !showPrompts; }}
      class="prompts-toggle"
    >
      <span style="font-family: var(--mono-product); font-size: 11px;">{showPrompts ? '▼' : '▶'}</span>
      Prompt Templates
    </button>
    {#if showPrompts}
      <div class="card-term" style="margin-top: 12px; padding: 20px;">
        <p style="font-family: var(--sans-product); font-size: 14px; color: var(--glow-mute); line-height: 1.7; margin: 0;">
          Prompt templates are defined in the backend orchestrator. Each round uses a structured
          prompt that includes the bot's constitutional role, the debate topic, prior responses
          (anonymised), and round-specific instructions. Templates are not user-configurable in
          this release.
        </p>
      </div>
    {/if}
  </section>
</div>
