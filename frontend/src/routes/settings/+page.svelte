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

<div class="max-w-4xl">
  <h1 class="mono text-2xl font-bold mb-2">Settings</h1>

  <!-- Banner -->
  <div class="bg-[#8b5cf6]/10 border border-[#8b5cf6]/30 rounded-lg p-4 mb-8">
    <p class="text-sm text-[#a78bfa]">
      Protocol configuration is read-only in this release. Changes require backend deployment.
    </p>
  </div>

  <!-- Protocol Parameters -->
  <section class="mb-8">
    <h2 class="mono text-lg font-bold text-[var(--text-primary)] mb-4">Protocol Parameters</h2>
    <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg divide-y divide-[var(--border)]">
      {#each [
        { label: 'Rounds', value: '5' },
        { label: 'Quorum', value: '3 bots minimum' },
        { label: 'Response timeout', value: '5 minutes' },
        { label: 'Max retries', value: '2' },
        { label: 'Synthesis model', value: 'claude-opus-4-6' },
        { label: 'Synthesis temperature', value: '0.0' },
      ] as param}
        <div class="flex items-center justify-between px-5 py-3">
          <span class="text-sm text-[var(--text-secondary)]">{param.label}</span>
          <span class="mono text-sm text-[var(--text-primary)]">{param.value}</span>
        </div>
      {/each}
    </div>
  </section>

  <!-- Constitutional Roles -->
  <section class="mb-8">
    <h2 class="mono text-lg font-bold text-[var(--text-primary)] mb-4">Constitutional Roles</h2>
    <div class="overflow-x-auto">
      <table class="w-full text-sm bg-[var(--surface)] border border-[var(--border)] rounded-lg">
        <thead>
          <tr class="border-b border-[var(--border)]">
            <th class="text-left py-3 px-5 text-xs mono text-[var(--text-muted)] font-normal">Role</th>
            <th class="text-left py-3 px-5 text-xs mono text-[var(--text-muted)] font-normal">Description</th>
            <th class="text-left py-3 px-5 text-xs mono text-[var(--text-muted)] font-normal">Enforcement Rule</th>
          </tr>
        </thead>
        <tbody>
          {#each ROLES as role}
            <tr class="border-b border-[var(--border)] last:border-0">
              <td class="py-3 px-5 mono text-xs text-[#8b5cf6] whitespace-nowrap">{role.name}</td>
              <td class="py-3 px-5 text-[var(--text-secondary)]">{role.description}</td>
              <td class="py-3 px-5 text-[var(--text-muted)] text-xs">{role.enforcement}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </section>

  <!-- Prompt Templates -->
  <section>
    <button
      onclick={() => { showPrompts = !showPrompts; }}
      class="flex items-center gap-2 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)] transition-colors"
    >
      <span class="mono text-xs">{showPrompts ? '\u25BC' : '\u25B6'}</span>
      Prompt Templates
    </button>
    {#if showPrompts}
      <div class="mt-3 bg-[var(--surface)] border border-[var(--border)] rounded-lg p-5">
        <p class="text-sm text-[var(--text-muted)]">
          Prompt templates are defined in the backend orchestrator. Each round uses a structured
          prompt that includes the bot's constitutional role, the debate topic, prior responses
          (anonymised), and round-specific instructions. Templates are not user-configurable in
          this release.
        </p>
      </div>
    {/if}
  </section>
</div>
