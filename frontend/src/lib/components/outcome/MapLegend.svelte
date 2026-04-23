<script lang="ts">
  // A compact rubric for the 3D argument map. The positions aren't
  // cosmetic — they encode what kind of claim each node is. Without this
  // legend the user sees a cloud; with it, the layout reads as a figure.
</script>

<div
  class="card-term"
  style="padding: 14px; margin-bottom: 12px;"
  aria-label="Argument map legend"
>
  <div class="flex flex-wrap items-center gap-x-5 gap-y-2" style="font-family: var(--mono-product); font-size: 11px; color: var(--glow-mute);">
    <div class="flex items-center gap-2">
      <span class="inline-block w-2.5 h-2.5 rounded-full" style="background: #f5f5f5;"></span>
      <span><span style="color: var(--glow-txt);">Topic</span> (centre) — the debate question</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="inline-block w-2.5 h-2.5 rounded-full" style="background: #10b981;"></span>
      <span><span style="color: #10b981;">Consensus</span> (top) — bots converged</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="inline-block w-2.5 h-2.5 rounded-full" style="background: #f43f5e;"></span>
      <span><span style="color: #f43f5e;">Contested</span> (left / right) — opposing sides</span>
    </div>
    <div class="flex items-center gap-2">
      <span class="inline-block w-2.5 h-2.5 rounded-full" style="background: #6366F1;"></span>
      <span><span style="color: #6366F1;">Minority</span> (bottom) — dissent preserved</span>
    </div>
  </div>

  <div class="flex flex-wrap items-center gap-x-5 gap-y-1 mt-2 pt-2" style="border-top: 1px solid var(--night-rule2); font-family: var(--mono-product); font-size: 10px; color: var(--glow-faint);">
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
