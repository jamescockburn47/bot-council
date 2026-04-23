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
  <!-- Backdrop -->
  <div
    class="fixed inset-0 z-40"
    style="background: rgba(8,8,13,0.75); backdrop-filter: blur(4px);"
    onclick={onClose}
    role="presentation"
  ></div>
  <div
    bind:this={panel}
    role="dialog"
    aria-modal="true"
    aria-label="Argument details"
    class="fixed top-0 right-0 h-full w-[360px] max-w-[92vw] z-50 overflow-y-auto shadow-2xl"
    style="background: var(--night-raise); border-left: 1px solid var(--night-rule2);"
  >
    <div
      class="sticky top-0 flex items-center justify-between px-5 py-3"
      style="background: var(--night-raise); border-bottom: 1px solid var(--night-rule2); backdrop-filter: blur(4px);"
    >
      <h2 class="tm-eyebrow" style="color: var(--glow-mute);">
        {disagreement ? 'Disagreement' : node ? kindLabel(node.kind) : ''}
      </h2>
      <button
        type="button"
        onclick={onClose}
        aria-label="Close details"
        class="btn-dark-ghost"
        style="padding: 4px 10px; font-size: 14px;"
      >
        ×
      </button>
    </div>

    <div class="px-5 py-4 text-sm">
      {#if disagreement}
        <p
          class="mono-label" style="margin-bottom: 4px;"
        >
          Issue
        </p>
        <p style="color: var(--glow-txt); margin-bottom: 20px; line-height: 1.6;">
          {disagreement.issue}
        </p>

        {#each [disagreement.sideA, disagreement.sideB] as side (side.id)}
          <div
            class="mb-5 pb-4 last:pb-0 last:mb-0" style="border-bottom: 1px solid var(--night-rule2);"
          >
            <p
              class="mono-label" style="margin-bottom: 4px; color: {colourFor(side.kind)};"
            >
              Side {side.sideKey?.toUpperCase()}
            </p>
            <p style="color: var(--glow-txt); margin-bottom: 8px; line-height: 1.6;">
              {side.fullText}
            </p>
            {#if side.bestArgument}
              <p style="font-size: 12px; color: var(--glow-dim); margin-bottom: 8px; font-style: italic; line-height: 1.6;">
                {side.bestArgument}
              </p>
            {/if}
            <p class="mono-label" style="margin-bottom: 8px;">
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
            class="mono-label" style="margin-bottom: 4px;"
          >
            Issue
          </p>
          <p style="font-size: 12px; color: var(--glow-dim); margin-bottom: 16px; line-height: 1.6;">
            {node.disagreementIssue}
          </p>
        {/if}

        <p
          class="mono-label" style="margin-bottom: 4px; color: {colourFor(node.kind)};"
        >
          {node.kind === 'topic' ? 'Topic' : 'Position'}
        </p>
        <p style="color: var(--glow-txt); margin-bottom: 20px; line-height: 1.6; font-size: 15px;">
          {node.fullText || '—'}
        </p>

        {#if node.bestArgument}
          <p
            class="mono-label" style="margin-bottom: 4px;"
          >
            Best argument
          </p>
          <p style="font-size: 13px; color: var(--glow-dim); margin-bottom: 20px; line-height: 1.6; font-style: italic;">
            {node.bestArgument}
          </p>
        {/if}

        {#if node.evidence}
          <p
            class="mono-label" style="margin-bottom: 4px;"
          >
            Evidence
          </p>
          <p style="font-size: 13px; color: var(--glow-dim); margin-bottom: 20px; line-height: 1.6;">
            {node.evidence}
          </p>
        {/if}

        <div class="grid grid-cols-2 gap-3 mb-4" style="font-size: 12px;">
          <div>
            <p class="mono-label">Support</p>
            <p style="color: var(--glow-txt);">
              {node.support} of {node.totalBots}
            </p>
          </div>
          {#if node.confidence != null}
            <div>
              <p class="mono-label">Confidence</p>
              <p style="color: var(--glow-txt);">{node.confidence}</p>
            </div>
          {/if}
        </div>

        {#if node.supporters.length > 0}
          <p class="mono-label" style="margin-bottom: 8px;">Supporters</p>
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
