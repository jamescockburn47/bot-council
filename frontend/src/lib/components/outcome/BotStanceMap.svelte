<script lang="ts">
  import type { SynthesisResponse, TranscriptResponse } from '$lib/types';

  /**
   * Bot-stance / reversal matrix — Graph 2 of the Outcome tab.
   *
   * The argument-map (Graph 1) shows what was argued. This shows who said
   * it and how their position moved over the rounds. Specifically, we
   * want reversals and unjustified capitulations to jump off the grid,
   * because those are the "shape of disagreement over time" that the
   * median-slop AI summary throws away.
   *
   * Each cell is (bot × round). Fill colour encodes confidence (green-
   * yellow-red gradient). An amber "↻" marks a position_change; a red
   * border + red "↻" marks a capitulation the synthesis flagged as
   * without adequate justification. Abstentions render as a dash.
   */

  let {
    transcript,
    synthesis,
  }: {
    transcript: TranscriptResponse;
    synthesis: SynthesisResponse;
  } = $props();

  const CELL_W = 90;
  const ROW_H = 48;
  const LEFT_PAD = 110;
  const TOP_PAD = 32;

  let bots = $derived(transcript.anonymisation_log.map((e) => e.pseudonym));
  let rounds = $derived(transcript.rounds);

  interface Cell {
    round: number;
    confidence: number | null;
    changed: boolean;
    abstained: boolean;
    absent: boolean;
    capitulation: boolean;
    capReason: string | null;
  }

  let grid = $derived.by<Cell[][]>(() => {
    const caps = synthesis.synthesis.flagged_capitulations ?? [];
    const capsByBot = new Map<string, { reason: string; justified: boolean }>();
    for (const c of caps) {
      capsByBot.set(c.bot, {
        reason: c.flag_reason ?? '',
        justified: c.justification_adequate,
      });
    }
    return bots.map((bot) =>
      rounds.map((round) => {
        const r = round.responses.find((resp) => resp.pseudonym === bot);
        const changed = r?.position_change?.changed ?? false;
        const cap = capsByBot.get(bot);
        return {
          round: round.round_number,
          confidence: r?.confidence ?? null,
          changed,
          abstained: r?.abstained ?? false,
          absent: r == null,
          // A bot is flagged as a capitulation once by the synthesis, not
          // per round. Attach the flag to any round where the bot actually
          // changed position — usually one of the later rounds.
          capitulation: !!cap && !cap.justified && changed,
          capReason: cap && !cap.justified && changed ? cap.reason : null,
        };
      }),
    );
  });

  let width = $derived(LEFT_PAD + rounds.length * CELL_W + 24);
  let height = $derived(TOP_PAD + bots.length * ROW_H + 16);

  function confidenceColour(c: number | null, abstained: boolean, absent: boolean): string {
    if (absent) return '#0F0F17';
    if (abstained || c == null) return '#1A1A26';
    // green → yellow → red as confidence falls
    const hue = Math.max(0, Math.min(120, (c / 100) * 120));
    return `hsl(${hue}, 55%, 38%)`;
  }
</script>

<div
  class="card-term"
  style="padding: 20px; overflow: auto;"
  aria-label="Bot positions across rounds"
>
  <div class="mb-3">
    <h3 class="tm-eyebrow" style="color: var(--indigo-400);">
      Positions & reversals
    </h3>
    <p style="font-family: var(--sans-product); font-size: 11px; color: var(--glow-dim); margin-top: 4px; max-width: 42rem;">
      Each row is a bot, each column a round. Cell colour is that bot's confidence in
      its own answer that round — greener is more certain, redder is less. A "↻" marks
      a round where the bot changed position. A red border marks a capitulation the
      synthesis flagged as <em>insufficiently justified</em> — the sycophantic-collapse
      pattern the protocol is designed to catch.
    </p>
  </div>

  <svg
    width={width}
    height={height}
    viewBox="0 0 {width} {height}"
    style="display: block; font-family: ui-monospace, SF Mono, monospace;"
  >
    <!-- Round headers -->
    {#each rounds as round, i}
      <text
        x={LEFT_PAD + i * CELL_W + CELL_W / 2}
        y={20}
        fill="rgba(255,255,255,0.50)"
        font-size="10"
        text-anchor="middle"
      >
        R{round.round_number}
      </text>
    {/each}

    <!-- Bot rows -->
    {#each bots as bot, bi}
      <text
        x={LEFT_PAD - 10}
        y={TOP_PAD + bi * ROW_H + ROW_H / 2 + 4}
        fill="rgba(255,255,255,0.92)"
        font-size="11"
        text-anchor="end"
      >
        {bot}
      </text>

      {#each grid[bi] as cell, ri}
        <g>
          <rect
            x={LEFT_PAD + ri * CELL_W + 2}
            y={TOP_PAD + bi * ROW_H + 2}
            width={CELL_W - 4}
            height={ROW_H - 4}
            fill={confidenceColour(cell.confidence, cell.abstained, cell.absent)}
            fill-opacity={cell.absent ? 0.25 : cell.abstained ? 0.35 : 0.7}
            stroke={cell.capitulation
              ? '#ef4444'
              : cell.changed
                ? '#f59e0b'
                : cell.absent
                  ? '#0F0F17'
                  : '#1A1A26'}
            stroke-width={cell.capitulation ? 2 : 1}
            rx="5"
          >
            <title>
              R{cell.round}: {cell.absent
                ? 'no response'
                : cell.abstained
                  ? 'abstained'
                  : cell.confidence != null
                    ? `confidence ${cell.confidence}${cell.changed ? ' — position changed' : ''}${cell.capitulation ? ` · flagged: ${cell.capReason ?? ''}` : ''}`
                    : 'confidence unknown'}
            </title>
          </rect>

          <text
            x={LEFT_PAD + ri * CELL_W + CELL_W / 2}
            y={TOP_PAD + bi * ROW_H + ROW_H / 2 + 4}
            fill="rgba(255,255,255,0.92)"
            font-size="12"
            font-weight="600"
            text-anchor="middle"
          >
            {cell.absent ? '·' : cell.abstained ? '—' : cell.confidence ?? '?'}
          </text>

          {#if cell.changed}
            <text
              x={LEFT_PAD + ri * CELL_W + 10}
              y={TOP_PAD + bi * ROW_H + 14}
              fill={cell.capitulation ? '#ef4444' : '#f59e0b'}
              font-size="12"
            >↻</text>
          {/if}
        </g>
      {/each}
    {/each}
  </svg>

  <div class="mt-4 flex flex-wrap gap-4" style="font-family: var(--mono-product); font-size: 10px; letter-spacing: 0.15em; text-transform: uppercase; color: var(--glow-mute);">
    <div class="flex items-center gap-2">
      <span class="w-3 h-3 rounded" style="background: hsl(120,55%,38%);"></span>
      high confidence
    </div>
    <div class="flex items-center gap-2">
      <span class="w-3 h-3 rounded" style="background: hsl(60,55%,38%);"></span> mid
    </div>
    <div class="flex items-center gap-2">
      <span class="w-3 h-3 rounded" style="background: hsl(0,55%,38%);"></span> low
    </div>
    <div class="flex items-center gap-2">
      <span style="color: #f59e0b;">↻</span> position change
    </div>
    <div class="flex items-center gap-2">
      <span style="color: #ef4444; border: 1px solid #ef4444; padding: 0 4px; border-radius: 3px;">↻</span>
      unjustified capitulation
    </div>
  </div>
</div>
