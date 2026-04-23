<script lang="ts">
  import type { CitationCheckResult, TranscriptResponse } from '$lib/types';

  interface Props {
    citationCheck: CitationCheckResult | null;
    transcript: TranscriptResponse | null;
  }
  let { citationCheck, transcript }: Props = $props();

  let expanded = $state(false);

  type Bucket = { extracted: number; authored: number; failed: number };
  type FieldCounts = {
    challenge: Bucket;
    position_change: Bucket;
    steelman: Bucket;
    crux_engagement: Bucket;
  };

  const emptyBucket = (): Bucket => ({ extracted: 0, authored: 0, failed: 0 });

  /** Carry-forwards + retries tallied per (bot, round). */
  type Reliability = {
    carriedForward: Array<{ pseudonym: string; round: number; from: number }>;
    retried: Array<{ pseudonym: string; round: number; retries: number }>;
    abstained: Array<{ pseudonym: string; round: number }>;
  };

  function tally(
    t: TranscriptResponse | null,
  ): { fields: FieldCounts; reliability: Reliability } {
    const fields: FieldCounts = {
      challenge: emptyBucket(),
      position_change: emptyBucket(),
      steelman: emptyBucket(),
      crux_engagement: emptyBucket(),
    };
    const rel: Reliability = {
      carriedForward: [],
      retried: [],
      abstained: [],
    };
    if (!t) return { fields: fields, reliability: rel };

    for (const round of t.rounds) {
      for (const e of round.responses) {
        if (e.abstained) {
          rel.abstained.push({ pseudonym: e.pseudonym, round: round.round_number });
          continue;
        }
        if (e.fallback_from_round != null) {
          rel.carriedForward.push({
            pseudonym: e.pseudonym,
            round: round.round_number,
            from: e.fallback_from_round,
          });
          // carry-forward responses carry no extractions
          continue;
        }
        if (e.retry_count > 0) {
          rel.retried.push({
            pseudonym: e.pseudonym,
            round: round.round_number,
            retries: e.retry_count,
          });
        }
        const meta = e.extraction_metadata ?? {};
        for (const key of [
          'challenge',
          'position_change',
          'steelman',
          'crux_engagement',
        ] as const) {
          const prov = (meta as Record<string, { source?: string } | null | undefined>)[key];
          if (!prov?.source) continue;
          if (prov.source === 'extracted') fields[key].extracted++;
          else if (prov.source === 'authored') fields[key].authored++;
          else if (prov.source === 'extraction_failed') fields[key].failed++;
        }
      }
    }
    return { fields, reliability: rel };
  }

  let stats = $derived(tally(transcript));

  let citationsSummary = $derived.by(() => {
    if (!citationCheck) return null;
    const invalid = citationCheck.citations_invalid.length;
    const total = citationCheck.citations_total;
    return {
      total,
      valid: citationCheck.citations_valid,
      invalid,
      allValid: invalid === 0,
      emptyTotal: total === 0,
    };
  });

  let issueCount = $derived.by(() => {
    let n = 0;
    if (citationsSummary && citationsSummary.invalid > 0) n += citationsSummary.invalid;
    n += stats.fields.challenge.failed;
    n += stats.fields.position_change.failed;
    n += stats.fields.steelman.failed;
    n += stats.fields.crux_engagement.failed;
    n += stats.reliability.carriedForward.length;
    n += stats.reliability.abstained.length;
    return n;
  });

  let fieldRows = $derived([
    { key: 'challenge', label: 'Challenge (R2)', bucket: stats.fields.challenge },
    {
      key: 'position_change',
      label: 'Position change (R4)',
      bucket: stats.fields.position_change,
    },
    { key: 'steelman', label: 'Steelman (R4)', bucket: stats.fields.steelman },
    {
      key: 'crux_engagement',
      label: 'Crux engagement (R3)',
      bucket: stats.fields.crux_engagement,
    },
  ]);
</script>

<div class="card-term mb-6" style="padding: 0; overflow: hidden;">
  <button
    onclick={() => (expanded = !expanded)}
    class="w-full flex items-center justify-between px-4 py-3 text-left transition-colors"
    style="background: var(--night-raise);"
    onmouseenter={(e) => (e.currentTarget.style.background = 'var(--night-edge)')}
    onmouseleave={(e) => (e.currentTarget.style.background = 'var(--night-raise)')}
  >
    <div class="flex items-center gap-3">
      <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 0;">Synthesis quality report</p>
      {#if citationsSummary}
        {#if citationsSummary.emptyTotal}
          <span
            class="mono-label"
            style="padding: 2px 6px; border-radius: var(--r-sm); background: var(--night-rule); color: var(--glow-faint);"
          >
            no citations
          </span>
        {:else if citationsSummary.allValid}
          <span
            class="mono-label"
            style="padding: 2px 6px; border-radius: var(--r-sm); color: #4ade80; background: rgba(34,197,94,0.10); border: 1px solid rgba(34,197,94,0.20);"
          >
            {citationsSummary.total} citations · all valid
          </span>
        {:else}
          <span
            class="mono-label"
            style="padding: 2px 6px; border-radius: var(--r-sm); color: #fbbf24; background: rgba(245,158,11,0.10); border: 1px solid rgba(245,158,11,0.20);"
          >
            {citationsSummary.invalid}/{citationsSummary.total} citations flagged
          </span>
        {/if}
      {/if}
      {#if issueCount > 0}
        <span
          class="mono-label"
          style="padding: 2px 6px; border-radius: var(--r-sm); color: #fbbf24; background: rgba(245,158,11,0.10); border: 1px solid rgba(245,158,11,0.20);"
        >
          {issueCount} integrity note{issueCount !== 1 ? 's' : ''}
        </span>
      {:else}
        <span
          class="mono-label"
          style="padding: 2px 6px; border-radius: var(--r-sm); color: #4ade80; background: rgba(34,197,94,0.10); border: 1px solid rgba(34,197,94,0.20);"
        >
          no issues flagged
        </span>
      {/if}
    </div>
    <span class="mono-label">{expanded ? '▲' : '▼'}</span>
  </button>

  {#if expanded}
    <div class="px-4 py-4 space-y-5" style="border-top: 1px solid var(--night-rule);">
      <!-- Citation check -->
      {#if citationsSummary}
        <div>
          <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 8px;">Citation verification</p>
          <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-dim); line-height: 1.65; margin-bottom: 8px;">
            The synthesiser cites specific bot-round attributions (e.g. <code>[Agent A, Round 2]</code>).
            Each citation is checked against the transcript: bot must exist, round must exist, bot must
            have responded in that round without abstaining.
          </p>
          <p class="mono-label">
            <span style="color: var(--glow-txt);">{citationsSummary.valid}</span> of
            <span style="color: var(--glow-txt);">{citationsSummary.total}</span> citations verified.
          </p>
          {#if citationsSummary.invalid > 0}
            <ul class="mono-label mt-2 space-y-1 list-disc list-inside" style="color: #fbbf24;">
              {#each citationCheck!.citations_invalid as bad}
                <li>
                  <span style="color: var(--glow-txt);">{bad.citation}</span>
                  — {bad.reason}
                  <span style="color: var(--glow-faint);">({bad.location})</span>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}

      <!-- Extraction outcomes -->
      <div style="border-top: 1px solid var(--night-rule); padding-top: 16px;">
        <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 8px;">Extraction outcomes</p>
        <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-dim); line-height: 1.65; margin-bottom: 8px;">
          Structured fields are recovered from the bot's prose by a separate model with source-quote
          verification. <span style="color: var(--glow-txt);">extracted</span> = field recovered and
          quote verified. <span style="color: var(--glow-txt);">authored</span> = bot emitted the
          field directly on the wire. <span style="color: var(--glow-txt);">failed</span> = the
          verifier refused the result (likely a hallucinated quote) and the field was dropped to avoid
          a lying source label.
        </p>
        <table class="text-xs mono w-full">
          <thead>
            <tr>
              <th class="text-left py-1 mono-label">field</th>
              <th class="text-right py-1 mono-label">extracted</th>
              <th class="text-right py-1 mono-label">authored</th>
              <th class="text-right py-1 mono-label">failed</th>
            </tr>
          </thead>
          <tbody>
            {#each fieldRows as row (row.key)}
              <tr style="border-top: 1px solid var(--night-rule);">
                <td class="py-1" style="color: var(--glow-dim);">{row.label}</td>
                <td class="py-1 text-right" style="color: var(--glow-txt);">
                  {row.bucket.extracted}
                </td>
                <td class="py-1 text-right" style="color: var(--glow-faint);">
                  {row.bucket.authored}
                </td>
                <td class="py-1 text-right" style="color: {row.bucket.failed > 0 ? '#fbbf24' : 'var(--glow-faint)'};">
                  {row.bucket.failed}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <!-- Reliability: carry-forwards + retries + abstentions -->
      <div style="border-top: 1px solid var(--night-rule); padding-top: 16px;">
        <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 8px;">Response reliability</p>
        <div class="grid grid-cols-3 gap-4 text-xs mono">
          <div>
            <div class="mono-label mb-1">Retries</div>
            <div style="color: var(--glow-txt);"><span class="stat-serif" style="font-size: 24px;">{stats.reliability.retried.length}</span></div>
            {#if stats.reliability.retried.length > 0}
              <ul class="mono-label mt-1 space-y-0.5">
                {#each stats.reliability.retried as r}
                  <li>{r.pseudonym}, R{r.round}</li>
                {/each}
              </ul>
            {/if}
          </div>
          <div>
            <div class="mono-label mb-1">Carried forward</div>
            <div style="color: {stats.reliability.carriedForward.length > 0 ? '#fbbf24' : 'var(--glow-txt)'};">
              <span class="stat-serif" style="font-size: 24px; color: inherit;">{stats.reliability.carriedForward.length}</span>
            </div>
            {#if stats.reliability.carriedForward.length > 0}
              <ul class="mono-label mt-1 space-y-0.5">
                {#each stats.reliability.carriedForward as c}
                  <li>{c.pseudonym}, R{c.round} ← R{c.from}</li>
                {/each}
              </ul>
            {/if}
          </div>
          <div>
            <div class="mono-label mb-1">Abstained</div>
            <div style="color: {stats.reliability.abstained.length > 0 ? '#fbbf24' : 'var(--glow-txt)'};">
              <span class="stat-serif" style="font-size: 24px; color: inherit;">{stats.reliability.abstained.length}</span>
            </div>
            {#if stats.reliability.abstained.length > 0}
              <ul class="mono-label mt-1 space-y-0.5">
                {#each stats.reliability.abstained as a}
                  <li>{a.pseudonym}, R{a.round}</li>
                {/each}
              </ul>
            {/if}
          </div>
        </div>
        <p class="mono-label mt-3 leading-relaxed" style="text-transform: none; letter-spacing: 0; font-size: 11px;">
          Retry = simplified-prompt attempt that succeeded. Carry-forward = two attempts failed, the
          bot's earlier-round position was preserved so synthesis could still cite a voice.
          Abstained = no prior content existed to carry forward either.
        </p>
      </div>
    </div>
  {/if}
</div>
