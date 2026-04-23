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
  class="card-term-lg"
  style="margin-bottom: 24px;"
  aria-label="Debate divergence signals"
>
  <div class="flex items-baseline justify-between mb-4">
    <h3 class="tm-eyebrow" style="color: var(--indigo-400);">
      Divergence signals
    </h3>
    <span
      class="mono-label"
      style="padding: 2px 8px; border-radius: 4px; background: {toneStyles[tone].bar}22; color: {toneStyles[tone].bar};"
    >
      {toneStyles[tone].label}
    </span>
  </div>

  <!-- Big divergence score bar — the headline number. -->
  <div class="mb-5">
    <div class="flex items-baseline justify-between mb-1.5">
      <span class="mono-label">Divergence score</span>
      <span class="stat-serif" style="font-size: 40px; color: {toneStyles[tone].bar};">
        {sig.divergenceScore}<span style="font-size: 16px; opacity: 0.6;">/100</span>
      </span>
    </div>
    <div style="height: 6px; border-radius: 999px; background: var(--night-edge); overflow: hidden;">
      <div
        style="height: 100%; border-radius: 999px; transition: width 700ms; width: {sig.divergenceScore}%; background: {toneStyles[tone].bar};"
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
    {@render signal('Minority voices', sig.minorityVoices, '#6366F1', 'bots that held their ground alone', false)}
  </div>
</div>

{#snippet signal(label: string, count: number, colour: string, help: string, emphasis: boolean)}
  <div
    style="background: {colour}0a; border: 1px solid {colour}{emphasis ? '55' : '22'}; border-radius: var(--r-lg); padding: 12px;"
    title={help}
  >
    <div style="font-family: var(--serif-editorial); font-weight: 600; font-size: 28px; color: {colour};">
      {count}
    </div>
    <div class="mono-label" style="margin-top: 2px;">
      {label}
    </div>
  </div>
{/snippet}
