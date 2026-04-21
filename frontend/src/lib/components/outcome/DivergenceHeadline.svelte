<script lang="ts">
  import type { SynthesisData } from '$lib/types';
  import { computeDivergence } from '$lib/argument-graph/divergence';

  let { synthesis }: { synthesis: SynthesisData | null | undefined } = $props();

  let sig = $derived(computeDivergence(synthesis));

  /// Colour-coded tone for the divergence score bar: muted green when the
  /// debate mostly settled, amber when unsettled, red when dominated by
  /// dissent/reversals. We don't hide any of it — the point is to read
  /// at-a-glance whether this is a "clean answer" debate or a "messy,
  /// interesting" one.
  let tone = $derived(
    sig.divergenceScore >= 60
      ? 'red'
      : sig.divergenceScore >= 30
        ? 'amber'
        : 'green',
  );

  const toneStyles: Record<string, { bar: string; label: string }> = {
    green: { bar: '#10b981', label: 'Largely settled' },
    amber: { bar: '#f59e0b', label: 'Mixed signals' },
    red: { bar: '#ef4444', label: 'Highly divergent' },
  };
</script>

<div
  class="bg-[var(--surface)] border border-[var(--border)] rounded-xl p-5 mb-6"
  aria-label="Debate divergence signals"
>
  <div class="flex items-baseline justify-between mb-4">
    <h3 class="text-xs mono uppercase tracking-wider text-[var(--text-muted)]">
      Divergence signals
    </h3>
    <span
      class="mono text-xs px-2 py-0.5 rounded"
      style="background: {toneStyles[tone].bar}22; color: {toneStyles[tone].bar};"
    >
      {toneStyles[tone].label}
    </span>
  </div>

  <!-- Big divergence score bar — the headline number. -->
  <div class="mb-5">
    <div class="flex items-baseline justify-between mb-1.5">
      <span class="text-[10px] mono uppercase tracking-wider text-[var(--text-muted)]">
        Divergence score
      </span>
      <span class="mono text-2xl font-bold" style="color: {toneStyles[tone].bar};">
        {sig.divergenceScore}<span class="text-sm opacity-60">/100</span>
      </span>
    </div>
    <div class="h-1.5 rounded-full bg-[var(--border)] overflow-hidden">
      <div
        class="h-full rounded-full transition-all duration-700"
        style="width: {sig.divergenceScore}%; background: {toneStyles[tone].bar};"
      ></div>
    </div>
  </div>

  <!-- Five signal tiles — all shown regardless of value so the shape of
       the debate reads as a whole, including the zeroes. -->
  <div class="grid grid-cols-2 sm:grid-cols-5 gap-3">
    {@render signal('Consensus', sig.consensus, '#10b981', 'points all bots agreed on', false)}
    {@render signal('Live disagreements', sig.disagreements, '#ef4444', 'unresolved at final round', false)}
    {@render signal('Reversals', sig.reversals, '#f59e0b', 'bots that changed position', false)}
    {@render signal(
      'Unjustified flips',
      sig.unjustifiedReversals,
      '#f43f5e',
      'reversals without adequate justification',
      sig.unjustifiedReversals > 0,
    )}
    {@render signal('Minority voices', sig.minorityVoices, '#8b5cf6', 'bots that held their ground alone', false)}
  </div>
</div>

{#snippet signal(label: string, count: number, colour: string, help: string, emphasis: boolean)}
  <div
    class="rounded-lg p-3 border"
    style="background: {colour}0a; border-color: {colour}{emphasis ? '55' : '22'};"
    title={help}
  >
    <div class="mono text-2xl font-bold" style="color: {colour};">
      {count}
    </div>
    <div class="text-[10px] mono uppercase tracking-wider text-[var(--text-secondary)] mt-0.5">
      {label}
    </div>
  </div>
{/snippet}
