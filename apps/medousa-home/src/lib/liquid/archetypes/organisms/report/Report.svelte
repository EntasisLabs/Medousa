<script lang="ts">
  /**
   * `report` organism — prose + nested charts in a figure grid.
   * Paste-first from ```report markdown. Body re-enters MarkdownContent so
   * nested chart placeholders hydrate like brief section bodies.
   */
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { renderInlineMarkdown } from "$lib/markdown";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(
    typeof node.props.subtitle === "string" ? node.props.subtitle : "",
  );
  const columns = $derived.by(() => {
    const raw = typeof node.props.columns === "string" ? node.props.columns.trim() : "2";
    if (raw === "1" || raw === "3") return raw;
    return "2";
  });
  const body = $derived(typeof node.props.body === "string" ? node.props.body : "");
</script>

{#if title || subtitle || body}
  <article
    class="liquid-report"
    data-columns={columns}
    style:--report-cols={columns}
  >
    {#if title || subtitle}
      <header class="liquid-report-header">
        {#if title}
          <h3 class="liquid-report-title">{@html renderInlineMarkdown(title)}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-report-subtitle">{@html renderInlineMarkdown(subtitle)}</p>
        {/if}
      </header>
    {/if}

    {#if body}
      <div class="liquid-report-body">
        <MarkdownContent
          content={body}
          titleByPath={ctx.titleByPath}
          openLinksInWeb={ctx.openLinksInWeb ?? false}
        />
      </div>
    {/if}
  </article>
{/if}

<style>
  .liquid-report {
    margin: 0;
    padding: 0.95rem 1rem 1.05rem;
    border-radius: 0.95rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 22%, transparent);
    background: color-mix(in srgb, var(--color-surface-50) 42%, transparent);
    box-shadow:
      0 1px 0 color-mix(in srgb, var(--color-surface-50) 70%, transparent) inset,
      0 8px 24px rgb(0 0 0 / 0.03);
    min-width: 0;
  }

  :global(html.dark) .liquid-report {
    background: color-mix(in srgb, var(--color-surface-900) 42%, transparent);
    border-color: color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
  }

  .liquid-report-header {
    margin-bottom: 0.85rem;
    padding-bottom: 0.7rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 20%, transparent);
  }

  /* Match chart title tokens — readable on vault cream and dark chat */
  .liquid-report-title {
    margin: 0;
    font-size: 1.2rem;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.25;
    color: rgb(var(--chart-fg));
  }

  .liquid-report-subtitle {
    margin: 0.35rem 0 0;
    font-size: 0.8125rem;
    line-height: 1.45;
    font-weight: 450;
    color: rgb(var(--chart-fg-muted));
  }

  /* Beat vault `.markdown-content h3` wash when the report host sits in preview */
  :global(.markdown-content) .liquid-report-title {
    margin: 0;
    color: rgb(var(--chart-fg));
    font-size: 1.2rem;
    font-weight: 700;
  }

  :global(.markdown-content) .liquid-report-subtitle {
    margin: 0.35rem 0 0;
    color: rgb(var(--chart-fg-muted));
  }

  .liquid-report-title :global(strong),
  .liquid-report-subtitle :global(strong) {
    font-weight: 750;
    color: inherit;
  }

  .liquid-report-body {
    min-width: 0;
  }

  .liquid-report-body :global(.markdown-content) {
    display: grid;
    grid-template-columns: repeat(var(--report-cols, 2), minmax(0, 1fr));
    column-gap: 0.95rem;
    row-gap: 1.15rem;
    font-size: 0.875rem;
    line-height: 1.55;
    color: rgb(var(--chart-fg-secondary));
  }

  /* Prose / headings full-bleed; chart embeds sit in the figure grid */
  .liquid-report-body :global(.markdown-content > *) {
    grid-column: 1 / -1;
    min-width: 0;
  }

  .liquid-report-body :global(.markdown-content > .liquid-md-embed[data-liquid-embed="chart"]),
  .liquid-report-body :global(.markdown-content > .liquid-md-host-chart) {
    grid-column: auto;
  }

  .liquid-report-body :global(.markdown-content > p) {
    margin: 0;
    color: rgb(var(--chart-fg-secondary));
  }

  .liquid-report-body :global(.markdown-content > h1),
  .liquid-report-body :global(.markdown-content > h2),
  .liquid-report-body :global(.markdown-content > h3),
  .liquid-report-body :global(.markdown-content > h4) {
    margin: 0.55rem 0 0;
    color: rgb(var(--chart-fg));
  }

  /* Nested chart chrome — same ink as standalone charts in vault */
  .liquid-report-body :global(.liquid-chart-title) {
    color: rgb(var(--chart-fg));
  }

  .liquid-report-body :global(.liquid-chart-description),
  .liquid-report-body :global(.liquid-chart-caption),
  .liquid-report-body :global(.liquid-chart-heatmap-col-label),
  .liquid-report-body :global(.liquid-chart-heatmap-row-label) {
    color: rgb(var(--chart-fg-muted));
  }

  .liquid-report-body :global(.liquid-chart) {
    margin: 0;
    height: 100%;
  }

  .liquid-report-body :global(.liquid-md-embed[data-liquid-embed="chart"]),
  .liquid-report-body :global(.liquid-md-host-chart) {
    min-width: 0;
  }

  @media (max-width: 640px) {
    .liquid-report-body :global(.markdown-content) {
      grid-template-columns: 1fr;
    }
  }
</style>
