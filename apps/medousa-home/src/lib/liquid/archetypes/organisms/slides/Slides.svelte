<script lang="ts">
  /**
   * `slides` organism — labeled 16:9 deck frames with atmosphere washes / images.
   */
  import { renderInlineMarkdown } from "$lib/markdown";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import {
    isSlideBgImage,
    isSlideWash,
    normalizeSlideScrim,
    normalizeSlideWash,
    SLIDE_WASH_INK,
    type SlideScrim,
    type SlideWash,
  } from "$lib/utils/markdownSlides";
  import { resolveLocalImagePreviewUrl } from "$lib/utils/vaultLocalImages";
  import FigureGridBody from "./FigureGridBody.svelte";

  type SlideLayout = "hero" | "split" | "stack";

  type SlideItem = {
    id: string;
    label: string;
    layout: SlideLayout;
    body: string;
    bg?: string;
    scrim?: SlideScrim;
  };

  type ResolvedAtmosphere = {
    wash: SlideWash;
    imageSrc: string | null;
    /** Raw path pending vault resolve (relative). */
    imageRaw: string | null;
    scrim: SlideScrim;
    ink: "dark" | "light";
  };

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const theme = $derived(
    normalizeSlideWash(
      typeof node.props.theme === "string" ? node.props.theme : "paper",
    ),
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
      const bg = typeof rec.bg === "string" ? rec.bg.trim() : undefined;
      const scrimRaw = typeof rec.scrim === "string" ? rec.scrim : undefined;
      const scrim = normalizeSlideScrim(scrimRaw, isSlideBgImage(bg));
      out.push({
        id: typeof rec.id === "string" && rec.id.trim() ? rec.id : `slide-${index + 1}`,
        label: label || `Slide ${index + 1}`,
        layout,
        body,
        ...(bg ? { bg } : {}),
        ...(scrim ? { scrim } : {}),
      });
    }
    return out;
  });

  function resolveAtmosphere(slide: SlideItem): ResolvedAtmosphere {
    const bg = slide.bg?.trim() ?? "";
    if (isSlideBgImage(bg)) {
      const isRemote = /^https?:\/\//i.test(bg);
      const scrim = slide.scrim ?? "none";
      // dark scrim → light ink; none/light scrim → dark ink (readable on bright photos).
      const ink: "dark" | "light" = scrim === "dark" ? "light" : "dark";
      return {
        wash: theme,
        imageSrc: isRemote ? bg : null,
        imageRaw: isRemote ? null : bg,
        scrim,
        ink,
      };
    }
    const wash = bg && isSlideWash(bg) ? normalizeSlideWash(bg) : theme;
    return {
      wash,
      imageSrc: null,
      imageRaw: null,
      scrim: slide.scrim ?? "none",
      ink: SLIDE_WASH_INK[wash],
    };
  }

  const atmospheres = $derived(slides.map(resolveAtmosphere));

  /** Resolved blob/asset URLs for vault-relative slide backgrounds. */
  let resolvedImages = $state<Record<string, string>>({});

  $effect(() => {
    const notePath = ctx.localImagePath ?? null;
    const pending = slides
      .map((slide, i) => ({ slide, atm: atmospheres[i]! }))
      .filter(({ atm }) => Boolean(atm.imageRaw));
    if (pending.length === 0) {
      resolvedImages = {};
      return;
    }
    let cancelled = false;
    void (async () => {
      const next: Record<string, string> = {};
      await Promise.all(
        pending.map(async ({ slide, atm }) => {
          const raw = atm.imageRaw!;
          const url = await resolveLocalImagePreviewUrl(raw, notePath);
          if (url) next[slide.id] = url;
        }),
      );
      if (!cancelled) resolvedImages = next;
    })();
    return () => {
      cancelled = true;
    };
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

  function bgImageUrl(slide: SlideItem, atm: ResolvedAtmosphere): string | null {
    if (atm.imageSrc) return atm.imageSrc;
    if (atm.imageRaw) return resolvedImages[slide.id] ?? null;
    return null;
  }
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
          {@const atm = atmospheres[i]!}
          {@const imgUrl = bgImageUrl(slide, atm)}
          <article
            class="liquid-slide"
            class:liquid-slide--hero={slide.layout === "hero"}
            class:liquid-slide--stack={slide.layout === "stack"}
            class:liquid-slide--split={slide.layout === "split"}
            class:liquid-slide--ink-light={atm.ink === "light"}
            class:liquid-slide--ink-dark={atm.ink === "dark"}
            class:liquid-slide--has-image={Boolean(imgUrl)}
            data-slide-id={slide.id}
            data-slide-label={slide.label}
            data-layout={slide.layout}
            data-bg={atm.wash}
            data-scrim={atm.scrim}
          >
            {#if imgUrl}
              <img
                class="liquid-slide-bg-image"
                src={imgUrl}
                alt=""
                aria-hidden="true"
                decoding="async"
              />
              {#if atm.scrim !== "none"}
                <div
                  class="liquid-slide-scrim"
                  class:liquid-slide-scrim--dark={atm.scrim === "dark"}
                  class:liquid-slide-scrim--light={atm.scrim === "light"}
                  aria-hidden="true"
                ></div>
              {/if}
            {/if}

            <div class="liquid-slide-content">
              {#if slide.layout === "hero"}
                <div class="liquid-slide-hero">
                  <p class="liquid-slide-kicker">{slide.label}</p>
                  <FigureGridBody body={slide.body} columns="1" flow="report" />
                </div>
              {:else if slide.layout === "stack"}
                <FigureGridBody body={slide.body} columns="1" flow="report" />
              {:else}
                <FigureGridBody
                  body={slide.body}
                  {columns}
                  flow="split"
                />
              {/if}
            </div>
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
    position: relative;
    isolation: isolate;
    aspect-ratio: 16 / 9;
    width: 100%;
    max-width: 100%;
    box-sizing: border-box;
    padding: 1.35rem 1.5rem;
    border-radius: 0.85rem;
    border: 1px solid #d6d3cd;
    overflow: hidden;
    min-width: 0;
    color: #111827;
    --chart-fg: 17 24 39;
    --chart-fg-secondary: 55 65 81;
    --chart-fg-muted: 107 114 128;
    background: #f8f7f4;
  }

  /* Named washes */
  .liquid-slide[data-bg="paper"] {
    background: #f8f7f4;
    border-color: #d6d3cd;
  }

  .liquid-slide[data-bg="mist"] {
    background: linear-gradient(145deg, #e8eef3 0%, #f4f6f8 48%, #dde5ec 100%);
    border-color: #c5d0da;
  }

  .liquid-slide[data-bg="dusk"] {
    background: linear-gradient(155deg, #1a2332 0%, #243044 42%, #2c3a52 100%);
    border-color: #3d4a5f;
  }

  .liquid-slide[data-bg="ink"] {
    background: #0f1115;
    border-color: #2a2f38;
  }

  .liquid-slide[data-bg="ember"] {
    background: linear-gradient(150deg, #1c1410 0%, #3a2218 45%, #6b3420 100%);
    border-color: #5c3a2a;
  }

  .liquid-slide--ink-light {
    color: #f9fafb;
    --chart-fg: 249 250 251;
    --chart-fg-secondary: 229 231 235;
    --chart-fg-muted: 156 163 175;
  }

  .liquid-slide--ink-dark {
    color: #111827;
    --chart-fg: 17 24 39;
    --chart-fg-secondary: 55 65 81;
    --chart-fg-muted: 107 114 128;
  }

  .liquid-slide-bg-image {
    position: absolute;
    inset: 0;
    z-index: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    pointer-events: none;
  }

  .liquid-slide-scrim {
    position: absolute;
    inset: 0;
    z-index: 1;
    pointer-events: none;
  }

  .liquid-slide-scrim--dark {
    background: linear-gradient(
      160deg,
      rgb(15 17 21 / 0.72) 0%,
      rgb(15 17 21 / 0.55) 55%,
      rgb(15 17 21 / 0.68) 100%
    );
  }

  .liquid-slide-scrim--light {
    background: linear-gradient(
      160deg,
      rgb(248 247 244 / 0.82) 0%,
      rgb(248 247 244 / 0.7) 55%,
      rgb(248 247 244 / 0.78) 100%
    );
  }

  .liquid-slide-content {
    position: relative;
    z-index: 2;
    min-width: 0;
    height: 100%;
    overflow: auto;
  }

  /* Title slide: bottom-left stack, big type, intentional empty stage to the right. */
  .liquid-slide--hero {
    padding: 1.6rem 1.85rem 1.85rem;
  }

  .liquid-slide-hero {
    height: 100%;
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    align-items: flex-start;
    gap: 0.55rem;
    max-width: min(36rem, 72%);
    padding-bottom: 0.15rem;
  }

  .liquid-slide-kicker {
    margin: 0;
    font-size: 0.7rem;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: rgb(var(--chart-fg-muted));
  }

  .liquid-slide-hero :global(.markdown-content) {
    min-width: 0;
    width: 100%;
  }

  .liquid-slide-hero :global(.markdown-content > :first-child) {
    margin-top: 0;
  }

  .liquid-slide-hero :global(.markdown-content > :last-child) {
    margin-bottom: 0;
  }

  .liquid-slide-hero :global(.markdown-content h1) {
    margin: 0 0 0.4rem;
    font-size: clamp(1.85rem, 4.6vw, 2.85rem);
    font-weight: 750;
    line-height: 1.08;
    letter-spacing: -0.035em;
  }

  .liquid-slide-hero :global(.markdown-content h2) {
    margin: 0 0 0.35rem;
    font-size: clamp(1.45rem, 3.4vw, 2.15rem);
    font-weight: 700;
    line-height: 1.12;
    letter-spacing: -0.03em;
  }

  .liquid-slide-hero :global(.markdown-content h3) {
    margin: 0 0 0.3rem;
    font-size: clamp(1.2rem, 2.4vw, 1.55rem);
    font-weight: 650;
    line-height: 1.2;
  }

  .liquid-slide-hero :global(.markdown-content p) {
    margin: 0.2rem 0 0;
    max-width: 34ch;
    font-size: clamp(0.95rem, 1.55vw, 1.12rem);
    line-height: 1.45;
    color: rgb(var(--chart-fg-secondary));
  }

  .liquid-slide--ink-dark :global(.markdown-content),
  .liquid-slide--ink-dark :global(.markdown-content p),
  .liquid-slide--ink-dark :global(.markdown-content li),
  .liquid-slide--ink-dark :global(.markdown-content em),
  .liquid-slide--ink-dark :global(.markdown-content strong) {
    color: #374151 !important;
  }

  .liquid-slide--ink-dark :global(.markdown-content h1),
  .liquid-slide--ink-dark :global(.markdown-content h2),
  .liquid-slide--ink-dark :global(.markdown-content h3),
  .liquid-slide--ink-dark :global(.markdown-content h4),
  .liquid-slide--ink-dark :global(.markdown-content h5),
  .liquid-slide--ink-dark :global(.markdown-content h6) {
    color: #111827 !important;
  }

  .liquid-slide--ink-light :global(.markdown-content),
  .liquid-slide--ink-light :global(.markdown-content p),
  .liquid-slide--ink-light :global(.markdown-content li),
  .liquid-slide--ink-light :global(.markdown-content em),
  .liquid-slide--ink-light :global(.markdown-content strong) {
    color: #e5e7eb !important;
  }

  .liquid-slide--ink-light :global(.markdown-content h1),
  .liquid-slide--ink-light :global(.markdown-content h2),
  .liquid-slide--ink-light :global(.markdown-content h3),
  .liquid-slide--ink-light :global(.markdown-content h4),
  .liquid-slide--ink-light :global(.markdown-content h5),
  .liquid-slide--ink-light :global(.markdown-content h6) {
    color: #f9fafb !important;
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
