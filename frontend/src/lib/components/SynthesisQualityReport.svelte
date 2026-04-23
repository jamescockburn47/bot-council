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

<div
  class="bg-[var(--surface)] border border-[var(--border)] rounded-lg mb-6 overflow-hidden"
>
  <button
    onclick={() => (expanded = !expanded)}
    class="w-full flex items-center justify-between px-4 py-3 text-left hover:bg-[var(--bg)] transition-colors"
  >
    <div class="flex items-center gap-3">
      <h3 class="text-xs mono uppercase tracking-wider text-[var(--text-primary)]">
        Synthesis quality report
      </h3>
      {#if citationsSummary}
        {#if citationsSummary.emptyTotal}
          <span class="text-[10px] mono px-1.5 py-0.5 rounded bg-[var(--border)] text-[var(--text-muted)]">
            no citations
          </span>
        {:else if citationsSummary.allValid}
          <span
            class="text-[10px] mono px-1.5 py-0.5 rounded bg-green-500/10 text-green-400 border border-green-500/20"
          >
            {citationsSummary.total} citations · all valid
          </span>
        {:else}
          <span
            class="text-[10px] mono px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20"
          >
            {citationsSummary.invalid}/{citationsSummary.total} citations flagged
          </span>
        {/if}
      {/if}
      {#if issueCount > 0}
        <span
          class="text-[10px] mono px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-400 border border-amber-500/20"
        >
          {issueCount} integrity note{issueCount !== 1 ? 's' : ''}
        </span>
      {:else}
        <span
          class="text-[10px] mono px-1.5 py-0.5 rounded bg-green-500/10 text-green-400 border border-green-500/20"
        >
          no issues flagged
        </span>
      {/if}
    </div>
    <span class="text-xs mono text-[var(--text-muted)]">
      {expanded ? '▲' : '▼'}
    </span>
  </button>

  {#if expanded}
    <div class="px-4 py-4 border-t border-[var(--border)] space-y-5">
      <!-- Citation check -->
      {#if citationsSummary}
        <div>
          <h4 class="text-[11px] mono uppercase tracking-wider text-[var(--text-muted)] mb-2">
            Citation verification
          </h4>
          <p class="text-xs text-[var(--text-secondary)] mb-2 leading-relaxed">
            The synthesiser cites specific bot-round attributions (e.g. <code>[Agent A, Round 2]</code>).
            Each citation is checked against the transcript: bot must exist, round must exist, bot must
            have responded in that round without abstaining.
          </p>
          <div class="text-xs mono text-[var(--text-secondary)]">
            <span class="text-[var(--text-primary)]">{citationsSummary.valid}</span> of
            <span class="text-[var(--text-primary)]">{citationsSummary.total}</span> citations
            verified.
          </div>
          {#if citationsSummary.invalid > 0}
            <ul class="text-xs mono text-amber-400 mt-2 space-y-1 list-disc list-inside">
              {#each citationCheck!.citations_invalid as bad}
                <li>
                  <span class="text-[var(--text-primary)]">{bad.citation}</span>
                  — {bad.reason}
                  <span class="text-[var(--text-muted)]">({bad.location})</span>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}

      <!-- Extraction outcomes -->
      <div>
        <h4 class="text-[11px] mono uppercase tracking-wider text-[var(--text-muted)] mb-2">
          Extraction outcomes
        </h4>
        <p class="text-xs text-[var(--text-secondary)] mb-2 leading-relaxed">
          Structured fields are recovered from the bot's prose by a separate model with source-quote
          verification. <span class="text-[var(--text-primary)]">extracted</span> = field recovered and
          quote verified. <span class="text-[var(--text-primary)]">authored</span> = bot emitted the
          field directly on the wire. <span class="text-[var(--text-primary)]">failed</span> = the
          verifier refused the result (likely a hallucinated quote) and the field was dropped to avoid
          a lying source label.
        </p>
        <table class="text-xs mono w-full">
          <thead>
            <tr class="text-[var(--text-muted)]">
              <th class="text-left py-1">field</th>
              <th class="text-right py-1">extracted</th>
              <th class="text-right py-1">authored</th>
              <th class="text-right py-1">failed</th>
            </tr>
          </thead>
          <tbody>
            {#each fieldRows as row (row.key)}
              <tr class="border-t border-[var(--border)]">
                <td class="py-1 text-[var(--text-secondary)]">{row.label}</td>
                <td class="py-1 text-right text-[var(--text-primary)]">
                  {row.bucket.extracted}
                </td>
                <td class="py-1 text-right text-[var(--text-muted)]">
                  {row.bucket.authored}
                </td>
                <td class="py-1 text-right {row.bucket.failed > 0 ? 'text-amber-400' : 'text-[var(--text-muted)]'}">
                  {row.bucket.failed}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <!-- Reliability: carry-forwards + retries + abstentions -->
      <div>
        <h4 class="text-[11px] mono uppercase tracking-wider text-[var(--text-muted)] mb-2">
          Response reliability
        </h4>
        <div class="grid grid-cols-3 gap-4 text-xs mono">
          <div>
            <div class="text-[var(--text-muted)] mb-1">Retries</div>
            <div class="text-[var(--text-primary)]">{stats.reliability.retried.length}</div>
            {#if stats.reliability.retried.length > 0}
              <ul class="text-[var(--text-muted)] mt-1 space-y-0.5">
                {#each stats.reliability.retried as r}
                  <li>{r.pseudonym}, R{r.round}</li>
                {/each}
              </ul>
            {/if}
          </div>
          <div>
            <div class="text-[var(--text-muted)] mb-1">Carried forward</div>
            <div class="text-{stats.reliability.carriedForward.length > 0 ? 'amber-400' : '[var(--text-primary)]'}">
              {stats.reliability.carriedForward.length}
            </div>
            {#if stats.reliability.carriedForward.length > 0}
              <ul class="text-[var(--text-muted)] mt-1 space-y-0.5">
                {#each stats.reliability.carriedForward as c}
                  <li>{c.pseudonym}, R{c.round} ← R{c.from}</li>
                {/each}
              </ul>
            {/if}
          </div>
          <div>
            <div class="text-[var(--text-muted)] mb-1">Abstained</div>
            <div class="text-{stats.reliability.abstained.length > 0 ? 'amber-400' : '[var(--text-primary)]'}">
              {stats.reliability.abstained.length}
            </div>
            {#if stats.reliability.abstained.length > 0}
              <ul class="text-[var(--text-muted)] mt-1 space-y-0.5">
                {#each stats.reliability.abstained as a}
                  <li>{a.pseudonym}, R{a.round}</li>
                {/each}
              </ul>
            {/if}
          </div>
        </div>
        <p class="text-[10px] text-[var(--text-muted)] mt-3 leading-relaxed">
          Retry = simplified-prompt attempt that succeeded. Carry-forward = two attempts failed, the
          bot's earlier-round position was preserved so synthesis could still cite a voice.
          Abstained = no prior content existed to carry forward either.
        </p>
      </div>
    </div>
  {/if}
</div>
