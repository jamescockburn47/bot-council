<script lang="ts">
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

</script>

<div
  class="card-term"
  style="margin-top: 12px; display: flex; align-items: center; gap: 12px; padding: 8px 12px;"
>
  <button
    type="button"
    onclick={onPlayToggle}
    disabled={rounds <= 1}
    class="btn-dark-ghost"
    style="font-family: var(--mono-product); font-size: 11px; letter-spacing: 0.1em; padding: 6px 12px; opacity: 1;"
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
        class="mono-label"
        style="padding: 4px 10px; border-radius: 6px; transition: all var(--dur-fast) var(--ease-standard); cursor: pointer; border: none;
               {round === t.id
                 ? 'background: var(--indigo-500); color: #fff; font-weight: 600;'
                 : 'background: transparent; color: var(--glow-faint);'}"
      >
        {t.label}
      </button>
      {#if t.id !== -1}
        <span style="height: 6px; width: 12px; background: var(--night-edge); border-radius: 999px; flex-shrink: 0;" aria-hidden="true"></span>
      {/if}
    {/each}
  </div>

  {#if inferred && round !== -1}
    <span
      class="mono-label"
      style="padding: 2px 8px; border-radius: 999px; background: rgba(245,158,11,0.1); color: #fcd34d; border: 1px solid rgba(245,158,11,0.2); white-space: nowrap;"
      title="Round data reconstructed heuristically from the transcript. Authoritative per-round synthesis is not yet available."
    >
      inferred
    </span>
  {/if}
</div>
