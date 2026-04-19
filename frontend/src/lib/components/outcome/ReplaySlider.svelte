<script lang="ts">
  import { onDestroy } from 'svelte';

  let {
    rounds,
    round,
    playing,
    inferred,
    onRoundChange,
    onPlayToggle,
  }: {
    rounds: number;
    // -1 means "Final" (terminal synthesis)
    round: number;
    playing: boolean;
    inferred: boolean;
    onRoundChange: (r: number) => void;
    onPlayToggle: () => void;
  } = $props();

  let ticks = $derived.by(() => {
    // Build an array of tick positions: 0..rounds-1 then "Final"
    const arr: { id: number; label: string }[] = [];
    for (let i = 0; i < rounds; i++) {
      arr.push({ id: i, label: `R${i}` });
    }
    arr.push({ id: -1, label: 'Final' });
    return arr;
  });

  let playTimer: ReturnType<typeof setInterval> | null = null;

  $effect(() => {
    if (playing) {
      playTimer = setInterval(() => {
        if (round === -1) {
          onPlayToggle();
          return;
        }
        const next = round + 1;
        if (next >= rounds) {
          onRoundChange(-1);
          onPlayToggle();
        } else {
          onRoundChange(next);
        }
      }, 1500);
    } else if (playTimer) {
      clearInterval(playTimer);
      playTimer = null;
    }
    return () => {
      if (playTimer) {
        clearInterval(playTimer);
        playTimer = null;
      }
    };
  });

  onDestroy(() => {
    if (playTimer) clearInterval(playTimer);
  });
</script>

<div
  class="mt-3 flex items-center gap-3 p-2 rounded-lg border border-[var(--border)] bg-[var(--surface)]"
>
  <button
    type="button"
    onclick={onPlayToggle}
    disabled={rounds <= 1}
    class="text-xs mono px-2.5 py-1 rounded border border-[var(--border)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:border-[var(--text-muted)] disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
    aria-label={playing ? 'Pause replay' : 'Play replay'}
  >
    {playing ? 'Pause' : 'Play'}
  </button>

  <div class="flex items-center gap-1 flex-1" role="tablist" aria-label="Replay round">
    {#each ticks as t (t.id)}
      <button
        type="button"
        role="tab"
        aria-selected={round === t.id}
        aria-label="Show round {t.label}"
        onclick={() => onRoundChange(t.id)}
        class="relative px-2.5 py-1 text-[10px] mono rounded transition-all
               {round === t.id
                 ? 'bg-[var(--text-primary)] text-[var(--bg)] font-medium'
                 : 'text-[var(--text-muted)] hover:text-[var(--text-primary)]'}"
      >
        {t.label}
      </button>
      {#if t.id !== -1}
        <span class="h-px w-3 bg-[var(--border)]" aria-hidden="true"></span>
      {/if}
    {/each}
  </div>

  {#if inferred && round !== -1}
    <span
      class="text-[10px] mono px-2 py-0.5 rounded-full bg-amber-500/10 text-amber-300 border border-amber-500/20 whitespace-nowrap"
      title="Round data reconstructed heuristically from the transcript. Authoritative per-round synthesis is not yet available."
    >
      inferred
    </span>
  {/if}
</div>
