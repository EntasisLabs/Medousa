<script lang="ts">
  /**
   * Markdown body with shared figure-column grid.
   * - report: prose full-bleed; chart/media/compare share columns
   * - split: prose + figures sit side-by-side in the column track
   */
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import "$lib/liquid/styles/liquidFigureGrid.css";

  interface Props {
    body: string;
    columns?: "1" | "2" | "3";
    /** report = prose full-bleed (default); split = prose beside figures */
    flow?: "report" | "split";
    class?: string;
  }

  let {
    body,
    columns = "2",
    flow = "report",
    class: className = "",
  }: Props = $props();
  const ctx = getLiquidContext();
</script>

{#if body}
  <div
    class="liquid-figure-grid-host {className}"
    class:liquid-figure-grid-host--split={flow === "split"}
    data-columns={columns}
    data-flow={flow}
    style:--figure-cols={columns}
  >
    <MarkdownContent
      content={body}
      titleByPath={ctx.titleByPath}
      openLinksInWeb={ctx.openLinksInWeb ?? false}
    />
  </div>
{/if}

<style>
  .liquid-figure-grid-host {
    min-width: 0;
  }

  .liquid-figure-grid-host :global(.markdown-content) {
    display: grid;
    grid-template-columns: repeat(var(--figure-cols, 2), minmax(0, 1fr));
    column-gap: 0.95rem;
    row-gap: 1.15rem;
    align-items: start;
    font-size: 0.875rem;
    line-height: 1.55;
    color: rgb(var(--chart-fg-secondary));
  }

  /* Report: everything full-bleed unless it's a figure embed */
  .liquid-figure-grid-host:not(.liquid-figure-grid-host--split)
    :global(.markdown-content > *) {
    grid-column: 1 / -1;
    min-width: 0;
  }

  .liquid-figure-grid-host:not(.liquid-figure-grid-host--split)
    :global(.markdown-content > .liquid-md-embed[data-liquid-embed="chart"]),
  .liquid-figure-grid-host:not(.liquid-figure-grid-host--split)
    :global(.markdown-content > .liquid-md-host-chart),
  .liquid-figure-grid-host:not(.liquid-figure-grid-host--split)
    :global(.markdown-content > .liquid-md-embed[data-liquid-embed="media"]),
  .liquid-figure-grid-host:not(.liquid-figure-grid-host--split)
    :global(.markdown-content > .liquid-md-host-media),
  .liquid-figure-grid-host:not(.liquid-figure-grid-host--split)
    :global(.markdown-content > .liquid-md-embed[data-liquid-embed="compare"]),
  .liquid-figure-grid-host:not(.liquid-figure-grid-host--split)
    :global(.markdown-content > .liquid-md-host-compare) {
    grid-column: auto;
  }

  /* Split: prose + figures share columns; only headings span full width */
  .liquid-figure-grid-host--split :global(.markdown-content > *) {
    grid-column: auto;
    min-width: 0;
  }

  .liquid-figure-grid-host--split :global(.markdown-content > h1),
  .liquid-figure-grid-host--split :global(.markdown-content > h2),
  .liquid-figure-grid-host--split :global(.markdown-content > h3),
  .liquid-figure-grid-host--split :global(.markdown-content > h4),
  .liquid-figure-grid-host--split :global(.markdown-content > h5),
  .liquid-figure-grid-host--split :global(.markdown-content > h6),
  .liquid-figure-grid-host--split :global(.markdown-content > hr) {
    grid-column: 1 / -1;
  }

  .liquid-figure-grid-host :global(.markdown-content > p) {
    margin: 0;
    color: rgb(var(--chart-fg-secondary));
  }

  .liquid-figure-grid-host--split :global(.markdown-content > p) {
    align-self: center;
  }

  .liquid-figure-grid-host :global(.markdown-content > h1),
  .liquid-figure-grid-host :global(.markdown-content > h2),
  .liquid-figure-grid-host :global(.markdown-content > h3),
  .liquid-figure-grid-host :global(.markdown-content > h4) {
    margin: 0.55rem 0 0;
    color: rgb(var(--chart-fg));
  }

  .liquid-figure-grid-host :global(.liquid-chart-title) {
    color: rgb(var(--chart-fg));
  }

  .liquid-figure-grid-host :global(.liquid-chart-description),
  .liquid-figure-grid-host :global(.liquid-chart-caption),
  .liquid-figure-grid-host :global(.liquid-chart-heatmap-col-label),
  .liquid-figure-grid-host :global(.liquid-chart-heatmap-row-label) {
    color: rgb(var(--chart-fg-muted));
  }

  .liquid-figure-grid-host :global(.liquid-chart) {
    margin: 0;
    height: 100%;
  }

  .liquid-figure-grid-host :global(.liquid-md-embed[data-liquid-embed="chart"]),
  .liquid-figure-grid-host :global(.liquid-md-host-chart) {
    min-width: 0;
  }

  @media (max-width: 640px) {
    .liquid-figure-grid-host :global(.markdown-content) {
      grid-template-columns: 1fr;
    }
  }
</style>
