<script lang="ts">
  /**
   * `report` organism — prose + nested charts in a figure grid.
   * Paste-first from ```report markdown. Body re-enters MarkdownContent so
   * nested chart placeholders hydrate like brief section bodies.
   */
  import { renderInlineMarkdown } from "$lib/markdown";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import FigureGridBody from "../slides/FigureGridBody.svelte";

  let { node }: ArchetypeProps = $props();

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
        <FigureGridBody {body} {columns} />
      </div>
    {/if}
  </article>
{/if}

<style>
  .liquid-report {
    margin: 0;
    padding: 0.95rem 1rem 1.05rem;
    border-radius: 0.95rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 42%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
    min-width: 0;
  }

  :global(html:not(.dark)) .liquid-report {
    background: color-mix(in srgb, var(--color-surface-50) 42%, transparent);
    border-color: color-mix(in srgb, var(--color-surface-500) 22%, transparent);
    box-shadow:
      0 1px 0 color-mix(in srgb, var(--color-surface-50) 70%, transparent) inset,
      0 8px 24px rgb(0 0 0 / 0.03);
  }

  :global(html:not(.dark) .vault-editor) .liquid-report {
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
</style>
