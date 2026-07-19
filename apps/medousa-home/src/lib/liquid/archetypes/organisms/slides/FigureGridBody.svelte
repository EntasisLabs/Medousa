<script lang="ts">
  /**
   * Markdown body with shared figure-column grid (prose full-bleed).
   */
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import "$lib/liquid/styles/liquidFigureGrid.css";

  interface Props {
    body: string;
    columns?: "1" | "2" | "3";
    class?: string;
  }

  let { body, columns = "2", class: className = "" }: Props = $props();
  const ctx = getLiquidContext();
</script>

{#if body}
  <div
    class="liquid-figure-grid-host {className}"
    data-columns={columns}
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

  /* Promote markdown root into the shared figure grid class. */
  .liquid-figure-grid-host :global(.markdown-content) {
    display: grid;
    grid-template-columns: repeat(var(--figure-cols, 2), minmax(0, 1fr));
    column-gap: 0.95rem;
    row-gap: 1.15rem;
    font-size: 0.875rem;
    line-height: 1.55;
    color: rgb(var(--chart-fg-secondary));
  }

  .liquid-figure-grid-host :global(.markdown-content > *) {
    grid-column: 1 / -1;
    min-width: 0;
  }

  .liquid-figure-grid-host
    :global(.markdown-content > .liquid-md-embed[data-liquid-embed="chart"]),
  .liquid-figure-grid-host :global(.markdown-content > .liquid-md-host-chart),
  .liquid-figure-grid-host
    :global(.markdown-content > .liquid-md-embed[data-liquid-embed="media"]),
  .liquid-figure-grid-host :global(.markdown-content > .liquid-md-host-media),
  .liquid-figure-grid-host
    :global(.markdown-content > .liquid-md-embed[data-liquid-embed="compare"]),
  .liquid-figure-grid-host :global(.markdown-content > .liquid-md-host-compare) {
    grid-column: auto;
  }

  .liquid-figure-grid-host :global(.markdown-content > p) {
    margin: 0;
    color: rgb(var(--chart-fg-secondary));
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
