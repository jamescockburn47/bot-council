<script lang="ts">
  // A compact rubric for the 3D argument map. The positions aren't
  // cosmetic — they encode what kind of claim each node is. Without this
  // legend the user sees a cloud; with it, the layout reads as a figure.
</script>

<div
  class="bg-[var(--surface)] border border-[var(--border)] rounded-xl p-3 mb-3 text-[11px] mono text-[var(--text-secondary)]"
  aria-label="Argument map legend"
>
  <div class="flex flex-wrap items-center gap-x-5 gap-y-2">
    <div class="flex items-center gap-2">
      <span class="inline-block w-2.5 h-2.5 rounded-full" style="background: #f5f5f5;"></span>
      <span><span class="text-[var(--text-primary)]">Topic</span> (centre) — the debate question</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="inline-block w-2.5 h-2.5 rounded-full" style="background: #10b981;"></span>
      <span><span class="text-[#10b981]">Consensus</span> (top) — bots converged</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="inline-block w-2.5 h-2.5 rounded-full" style="background: #f43f5e;"></span>
      <span><span class="text-[#f43f5e]">Contested</span> (left / right) — opposing sides</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="inline-block w-2.5 h-2.5 rounded-full" style="background: #8b5cf6;"></span>
      <span><span class="text-[#8b5cf6]">Minority</span> (bottom) — dissent preserved</span>
    </div>
  </div>

  <div class="flex flex-wrap items-center gap-x-5 gap-y-1 mt-2 pt-2 border-t border-[var(--border)] text-[10px] text-[var(--text-muted)]">
    <div class="flex items-center gap-2">
      <svg width="26" height="8" viewBox="0 0 26 8" aria-hidden="true">
        <line x1="0" y1="4" x2="26" y2="4" stroke="rgba(255,255,255,0.5)" stroke-width="1.3" />
      </svg>
      <span>Line to the topic — this node responds to the question</span>
    </div>
    <div class="flex items-center gap-2">
      <svg width="26" height="8" viewBox="0 0 26 8" aria-hidden="true">
        <line
          x1="0"
          y1="4"
          x2="26"
          y2="4"
          stroke="rgba(244,63,94,0.9)"
          stroke-width="1.3"
          stroke-dasharray="3 3"
        />
      </svg>
      <span>Tension between two nodes — opposing sides of the same disagreement</span>
    </div>
  </div>
</div>
