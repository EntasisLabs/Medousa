<script lang="ts">
  /**
   * `slides` organism — labeled 16:9 deck frames with report-style figure grid.
   */
  import { renderInlineMarkdown } from "$lib/markdown";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import FigureGridBody from "./FigureGridBody.svelte";

  type SlideLayout = "hero" | "split" | "stack";

  type SlideItem = {
    id: string;
    label: string;
    layout?: SlideLayout;
    body: string;
  };

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const theme = $derived(
    typeof node.props.theme === "string" ? node.props.theme.trim() || "paper" : "paper",
  );
  const columns = $derived.by(() => {
    const raw = typeof node.props.columns === "string" ? node.props.columns.trim() : "2";
    if (raw === "1" || raw === "3") return raw as "1" | "2" | "3";
    return "2";
  });
  const slides = $derived.by((): SlideItem[] => {
    const raw = node.props.slides;
    if (!Array.isArray(raw)) return [];
    const out: SlideItem[] = [];
    for (let index = 0; index < raw.length; index++) {
      const item = raw[index];
      if (!item || typeof item !== "object") continue;
      const rec = item as Record<string, unknown>;
      const label = typeof rec.label === "string" ? rec.label.trim() : "";
      const body = typeof rec.body === "string" ? rec.body : "";
      if (!label && !body.trim()) continue;
      const layoutRaw = typeof rec.layout === "string" ? rec.layout.trim() : "split";
      const layout: SlideLayout =
        layoutRaw === "hero" || layoutRaw === "stack" || layoutRaw === "split"
          ? layoutRaw
          : "split";
      out.push({
        id: typeof rec.id === "string" && rec.id.trim() ? rec.id : `slide-${index + 1}`,
        label: label || `Slide ${index + 1}`,
        layout,
        body,
      });
    }
    return out;
  });

  const activeIndex = $derived.by(() => {
    const raw = node.props.active;
    if (typeof raw === "number" && Number.isFinite(raw)) {
      return Math.max(0, Math.min(slides.length - 1, Math.floor(raw)));
    }
    if (typeof raw === "string" && raw.trim()) {
      const byId = slides.findIndex((s) => s.id === raw.trim());
      if (byId >= 0) return byId;
      const byLabel = slides.findIndex(
        (s) => s.label.toLowerCase() === raw.trim().toLowerCase(),
      );
      if (byLabel >= 0) return byLabel;
    }
    return 0;
  });

  let selected = $state(0);
  $effect(() => {
    void slides.length;
    void activeIndex;
    selected = activeIndex;
  });

  const exportPaper = $derived(
    Boolean(node.props.exportPaper) || Boolean(ctx.exportPaper),
  );
  const showAll = $derived(exportPaper || Boolean(node.props.showAll));
</script>

{#if slides.length > 0}
  <section
    class="liquid-slides"
    class:liquid-slides--export={exportPaper}
    data-theme={theme}
    data-columns={columns}
    style:--figure-cols={columns}
  >
    {#if title}
      <header class="liquid-slides-header">
        <h3 class="liquid-slides-title">{@html renderInlineMarkdown(title)}</h3>
      </header>
    {/if}

    {#if !showAll && slides.length > 1}
      <div class="liquid-slides-tabs" role="tablist" aria-label="Slides">
        {#each slides as slide, i (slide.id)}
          <button
            type="button"
            class="liquid-slides-tab"
            class:liquid-slides-tab--active={i === selected}
            role="tab"
            aria-selected={i === selected}
            onclick={() => (selected = i)}
          >
            {slide.label}
          </button>
        {/each}
      </div>
    {/if}

    <div class="liquid-slides-stage">
      {#each slides as slide, i (slide.id)}
        {#if showAll || i === selected}
          <article
            class="liquid-slide"
            class:liquid-slide--hero={slide.layout === "hero"}
            class:liquid-slide--stack={slide.layout === "stack"}
            class:liquid-slide--split={slide.layout === "split"}
            data-slide-id={slide.id}
            data-slide-label={slide.label}
            data-layout={slide.layout}
          >
            {#if slide.layout === "hero"}
              <div class="liquid-slide-hero">
                <p class="liquid-slide-kicker">{slide.label}</p>
                <FigureGridBody body={slide.body} columns="1" />
              </div>
            {:else}
              <FigureGridBody
                body={slide.body}
                columns={slide.layout === "stack" ? "1" : columns}
              />
            {/if}
          </article>
          {#if showAll && i < slides.length - 1}
            <div class="vault-export-page-break" aria-hidden="true"></div>
          {/if}
        {/if}
      {/each}
    </div>
  </section>
{/if}

<style>
  .liquid-slides {
    margin: 0;
    min-width: 0;
  }

  .liquid-slides-header {
    margin-bottom: 0.75rem;
  }

  .liquid-slides-title {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: rgb(var(--chart-fg));
  }

  :global(.markdown-content) .liquid-slides-title {
    margin: 0;
    color: rgb(var(--chart-fg));
  }

  .liquid-slides-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin-bottom: 0.75rem;
  }

  .liquid-slides-tab {
    appearance: none;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 30%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 35%, transparent);
    color: rgb(var(--chart-fg-muted));
    border-radius: 0.55rem;
    padding: 0.28rem 0.65rem;
    font-size: 0.75rem;
    font-weight: 550;
    cursor: pointer;
  }

  .liquid-slides-tab--active {
    color: rgb(var(--chart-fg));
    border-color: color-mix(in srgb, var(--color-surface-400) 45%, transparent);
    background: color-mix(in srgb, var(--color-surface-800) 55%, transparent);
  }

  .liquid-slides-stage {
    min-width: 0;
  }

  .liquid-slide {
    aspect-ratio: 16 / 9;
    width: 100%;
    max-width: 100%;
    box-sizing: border-box;
    padding: 1.35rem 1.5rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 42%, transparent);
    overflow: auto;
    min-width: 0;
  }

  /* Paper frame always uses print ink — even when the shell is dark. */
  :global(html:not(.dark)) .liquid-slide,
  .liquid-slides[data-theme="paper"] .liquid-slide {
    background: #f8f7f4;
    border-color: #d6d3cd;
    color: #111827;
    /* Remap chart/prose tokens so nested FigureGridBody stays readable. */
    --chart-fg: 17 24 39;
    --chart-fg-secondary: 55 65 81;
    --chart-fg-muted: 107 114 128;
  }

  :global(html:not(.dark) .vault-editor) .liquid-slides:not([data-theme="paper"]) .liquid-slide {
    background: color-mix(in srgb, var(--color-surface-900) 42%, transparent);
    border-color: color-mix(in srgb, var(--color-surface-500) 28%, transparent);
  }

  .liquid-slide-hero {
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0.75rem;
  }

  .liquid-slide-kicker {
    margin: 0;
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--chart-fg-muted));
  }

  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content),
  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content p),
  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content li),
  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content em),
  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content strong) {
    color: #374151 !important;
  }

  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content h1),
  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content h2),
  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content h3),
  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content h4),
  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content h5),
  .liquid-slides[data-theme="paper"] .liquid-slide :global(.markdown-content h6) {
    color: #111827 !important;
  }

  .liquid-slides[data-theme="paper"] .liquid-slide-kicker {
    color: #6b7280;
  }

  .liquid-slides--export .liquid-slides-tabs {
    display: none;
  }

  .liquid-slides--export .liquid-slide {
    break-inside: avoid;
    page-break-inside: avoid;
  }

  :global(.vault-pdf-export-mount) .vault-export-page-break {
    break-after: page;
    page-break-after: always;
    height: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }
</style>
