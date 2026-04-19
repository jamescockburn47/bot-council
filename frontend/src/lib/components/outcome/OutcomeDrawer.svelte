<script lang="ts">
  import type { GraphNode } from '$lib/argument-graph/types';
  import { colourFor } from '$lib/argument-graph/types';

  let {
    node = null,
    disagreement = null,
    onClose,
  }: {
    node?: GraphNode | null;
    disagreement?: { issue: string; sideA: GraphNode; sideB: GraphNode } | null;
    onClose: () => void;
  } = $props();

  let open = $derived(node !== null || disagreement !== null);

  // Focus trap
  let panel: HTMLDivElement | undefined = $state();
  $effect(() => {
    if (open && panel) {
      const prev = document.activeElement as HTMLElement | null;
      const focusable = panel.querySelector<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      );
      focusable?.focus();
      return () => prev?.focus();
    }
  });

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  function kindLabel(kind: string): string {
    switch (kind) {
      case 'topic':
        return 'Topic';
      case 'consensus':
        return 'Consensus';
      case 'contested':
        return 'Contested';
      case 'minority':
        return 'Minority';
    }
    return kind;
  }
</script>

<svelte:window onkeydown={onKey} />

{#if open}
  <div
    bind:this={panel}
    role="dialog"
    aria-modal="true"
    aria-label="Argument details"
    class="fixed top-0 right-0 h-full w-[360px] max-w-[92vw] z-50 bg-[var(--surface)]/95 backdrop-blur-md border-l border-[var(--border)] shadow-2xl overflow-y-auto"
  >
    <div
      class="sticky top-0 flex items-center justify-between px-5 py-3 border-b border-[var(--border)] bg-[var(--surface)]/90 backdrop-blur-md"
    >
      <h2 class="text-xs mono uppercase tracking-wider text-[var(--text-muted)]">
        {disagreement ? 'Disagreement' : node ? kindLabel(node.kind) : ''}
      </h2>
      <button
        type="button"
        onclick={onClose}
        aria-label="Close details"
        class="text-xs mono text-[var(--text-muted)] hover:text-[var(--text-primary)] transition-colors"
      >
        Close ✕
      </button>
    </div>

    <div class="px-5 py-4 text-sm">
      {#if disagreement}
        <p
          class="text-[10px] mono uppercase tracking-wider text-[var(--text-muted)] mb-1"
        >
          Issue
        </p>
        <p class="text-[var(--text-primary)] mb-5 leading-relaxed">
          {disagreement.issue}
        </p>

        {#each [disagreement.sideA, disagreement.sideB] as side (side.id)}
          <div
            class="mb-5 pb-4 border-b border-[var(--border)] last:border-0 last:pb-0 last:mb-0"
          >
            <p
              class="text-[10px] mono uppercase tracking-wider mb-1"
              style="color: {colourFor(side.kind)};"
            >
              Side {side.sideKey?.toUpperCase()}
            </p>
            <p class="text-[var(--text-primary)] mb-2 leading-relaxed">
              {side.fullText}
            </p>
            {#if side.bestArgument}
              <p class="text-xs text-[var(--text-secondary)] mb-2 italic leading-relaxed">
                {side.bestArgument}
              </p>
            {/if}
            <p class="text-[10px] mono text-[var(--text-muted)] mb-2">
              {side.support} of {side.totalBots}
            </p>
            <div class="flex flex-wrap gap-1">
              {#each side.supporters as p (p)}
                <span
                  class="text-[10px] mono px-2 py-0.5 rounded-full"
                  style="background: {colourFor(side.kind)}1a; color: {colourFor(side.kind)};"
                >
                  {p}
                </span>
              {/each}
            </div>
          </div>
        {/each}
      {:else if node}
        {#if node.disagreementIssue && node.kind === 'contested'}
          <p
            class="text-[10px] mono uppercase tracking-wider text-[var(--text-muted)] mb-1"
          >
            Issue
          </p>
          <p class="text-xs text-[var(--text-secondary)] mb-4 leading-relaxed">
            {node.disagreementIssue}
          </p>
        {/if}

        <p
          class="text-[10px] mono uppercase tracking-wider mb-1"
          style="color: {colourFor(node.kind)};"
        >
          {node.kind === 'topic' ? 'Topic' : 'Position'}
        </p>
        <p class="text-[var(--text-primary)] mb-5 leading-relaxed text-base">
          {node.fullText || '—'}
        </p>

        {#if node.bestArgument}
          <p
            class="text-[10px] mono uppercase tracking-wider text-[var(--text-muted)] mb-1"
          >
            Best argument
          </p>
          <p class="text-sm text-[var(--text-secondary)] mb-5 leading-relaxed italic">
            {node.bestArgument}
          </p>
        {/if}

        {#if node.evidence}
          <p
            class="text-[10px] mono uppercase tracking-wider text-[var(--text-muted)] mb-1"
          >
            Evidence
          </p>
          <p class="text-sm text-[var(--text-secondary)] mb-5 leading-relaxed">
            {node.evidence}
          </p>
        {/if}

        <div class="grid grid-cols-2 gap-3 mb-4 text-xs">
          <div>
            <p class="text-[10px] mono uppercase tracking-wider text-[var(--text-muted)]">
              Support
            </p>
            <p class="text-[var(--text-primary)]">
              {node.support} of {node.totalBots}
            </p>
          </div>
          {#if node.confidence != null}
            <div>
              <p class="text-[10px] mono uppercase tracking-wider text-[var(--text-muted)]">
                Confidence
              </p>
              <p class="text-[var(--text-primary)]">{node.confidence}</p>
            </div>
          {/if}
        </div>

        {#if node.supporters.length > 0}
          <p
            class="text-[10px] mono uppercase tracking-wider text-[var(--text-muted)] mb-2"
          >
            Supporters
          </p>
          <div class="flex flex-wrap gap-1">
            {#each node.supporters as p (p)}
              <span
                class="text-[10px] mono px-2 py-0.5 rounded-full"
                style="background: {colourFor(node.kind)}1a; color: {colourFor(node.kind)};"
              >
                {p}
              </span>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  </div>
{/if}
