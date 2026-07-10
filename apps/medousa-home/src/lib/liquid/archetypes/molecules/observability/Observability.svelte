<script lang="ts">
  /**
   * Collapsed-by-default drawer for thinking + tool lineage.
   * Substance stays above; this is on-demand observability.
   */
  import Slot from "$lib/liquid/render/Slot.svelte";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();

  const summary = $derived(
    typeof node.props.summary === "string" && node.props.summary.trim()
      ? node.props.summary.trim()
      : "How this was made",
  );
  const collapsed = $derived(node.props.collapsed !== false);
  const streaming = $derived(node.props.streaming === true);
  const detail = $derived(node.slots?.detail ?? []);
</script>

{#if detail.length > 0}
  <details class="liquid-obs" open={!collapsed || streaming}>
    <summary class="liquid-obs-summary">
      <span class="liquid-obs-label">{summary}</span>
      <span class="liquid-obs-chevron" aria-hidden="true">▾</span>
    </summary>
    <div class="liquid-obs-detail">
      <Slot nodes={detail} />
    </div>
  </details>
{/if}

<style>
  .liquid-obs {
    margin-top: 0.75rem;
    min-width: 0;
  }

  .liquid-obs-summary {
    display: flex;
    cursor: pointer;
    list-style: none;
    align-items: center;
    gap: 0.35rem;
    padding: 0.15rem 0;
    color: rgb(var(--color-surface-500));
    font-size: 0.625rem;
    letter-spacing: 0.02em;
    user-select: none;
  }

  .liquid-obs-summary::-webkit-details-marker {
    display: none;
  }

  .liquid-obs-summary:hover {
    color: rgb(var(--color-surface-300));
  }

  .liquid-obs-label {
    min-width: 0;
    flex: 1;
  }

  .liquid-obs-chevron {
    color: rgb(var(--color-surface-600));
    font-size: 0.7rem;
    line-height: 1;
    transition: transform 0.15s ease;
  }

  .liquid-obs[open] .liquid-obs-chevron {
    transform: rotate(180deg);
  }

  .liquid-obs-detail {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin-top: 0.4rem;
    padding-top: 0.4rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-600) 35%, transparent);
  }
</style>
