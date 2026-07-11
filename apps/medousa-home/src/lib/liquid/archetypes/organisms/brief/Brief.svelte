<script lang="ts">
  /**
   * `brief` organism — structured written answer WITH sources.
   * Anti-Wikipedia: judgment + citations, not a wall of anonymous prose.
   * Paste-first from ```brief markdown. Section bodies re-enter the markdown
   * pipeline so nested ```cite / emphasis / lists hydrate correctly.
   */
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { renderInlineMarkdown } from "$lib/markdown";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { createSceneEvent } from "$lib/liquid/core";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { openInBrowser, isHttpUrl } from "$lib/utils/openInBrowser";

  interface BriefSection {
    id: string;
    heading: string;
    body: string;
  }

  interface BriefSource {
    id: string;
    title: string;
    url?: string;
    quote?: string;
  }

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const subtitle = $derived(typeof node.props.subtitle === "string" ? node.props.subtitle : "");
  const tone = $derived(typeof node.props.tone === "string" ? node.props.tone.trim().toLowerCase() : "");

  const sections = $derived.by((): BriefSection[] => {
    const raw = node.props.sections;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const heading = typeof row.heading === "string" ? row.heading.trim() : "";
        const body = typeof row.body === "string" ? row.body.trim() : "";
        if (!heading || !body) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `section-${i}`;
        return { id, heading, body };
      })
      .filter((s): s is BriefSection => s !== null);
  });

  const sources = $derived.by((): BriefSource[] => {
    const raw = node.props.sources;
    if (!Array.isArray(raw)) return [];
    return raw
      .map((item, i) => {
        if (!item || typeof item !== "object") return null;
        const row = item as Record<string, unknown>;
        const titleText = typeof row.title === "string" ? row.title.trim() : "";
        if (!titleText) return null;
        const id = typeof row.id === "string" && row.id ? row.id : `source-${i}`;
        const src: BriefSource = { id, title: titleText };
        if (typeof row.url === "string" && row.url.trim()) src.url = row.url.trim();
        if (typeof row.quote === "string" && row.quote.trim()) src.quote = row.quote.trim();
        return src;
      })
      .filter((s): s is BriefSource => s !== null);
  });

  function selectSource(src: BriefSource, event: MouseEvent) {
    ctx.sink?.emit(
      createSceneEvent(node.id, "select", { sourceId: src.id, title: src.title, url: src.url }),
    );
    if (src.url && isHttpUrl(src.url)) {
      ctx.sink?.emit(
        createSceneEvent(node.id, "navigate", { sourceId: src.id, url: src.url }),
      );
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.button === 1) return;
      if (ctx.openLinksInWeb) {
        event.preventDefault();
        void openInBrowser(src.url, { openedBy: "user" });
      }
    }
  }
</script>

{#if sections.length >= 1}
  <article class="liquid-brief" data-tone={tone || undefined}>
    {#if title || subtitle || tone}
      <header class="liquid-brief-header">
        {#if title}
          <h3 class="liquid-brief-title">{@html renderInlineMarkdown(title)}</h3>
        {/if}
        {#if subtitle}
          <p class="liquid-brief-subtitle">{@html renderInlineMarkdown(subtitle)}</p>
        {/if}
        {#if tone === "research" || tone === "brief" || tone === "memo"}
          <p class="liquid-brief-tone">{tone}</p>
        {/if}
      </header>
    {/if}

    <div class="liquid-brief-sections">
      {#each sections as section (section.id)}
        <section class="liquid-brief-section">
          <h4 class="liquid-brief-heading">{@html renderInlineMarkdown(section.heading)}</h4>
          <div class="liquid-brief-body">
            <MarkdownContent
              content={section.body}
              titleByPath={ctx.titleByPath}
              openLinksInWeb={ctx.openLinksInWeb ?? false}
            />
          </div>
        </section>
      {/each}
    </div>

    {#if sources.length}
      <footer class="liquid-brief-sources">
        <p class="liquid-brief-sources-label">Sources</p>
        <ol class="liquid-brief-source-list">
          {#each sources as src, i (src.id)}
            <li class="liquid-brief-source">
              {#if src.url && isHttpUrl(src.url)}
                <a
                  class="liquid-brief-source-link"
                  href={src.url}
                  target="_blank"
                  rel="noopener noreferrer"
                  onclick={(e) => selectSource(src, e)}
                >
                  <span class="liquid-brief-source-index">{i + 1}.</span>
                  <span class="liquid-brief-source-title">{src.title}</span>
                </a>
              {:else}
                <button
                  type="button"
                  class="liquid-brief-source-plain"
                  onclick={(e) => selectSource(src, e)}
                >
                  <span class="liquid-brief-source-index">{i + 1}.</span>
                  <span class="liquid-brief-source-title">{src.title}</span>
                </button>
              {/if}
              {#if src.quote}
                <p class="liquid-brief-source-quote">{src.quote}</p>
              {/if}
            </li>
          {/each}
        </ol>
      </footer>
    {/if}
  </article>
{/if}

<style>
  .liquid-brief {
    margin: 0;
    padding: 0.9rem 0.95rem 1rem;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
    min-width: 0;
  }

  .liquid-brief-header {
    margin-bottom: 0.85rem;
    padding-bottom: 0.7rem;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-500) 22%, transparent);
  }

  .liquid-brief-title {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 700;
    letter-spacing: -0.02em;
    color: rgb(var(--color-surface-50));
  }

  .liquid-brief-subtitle {
    margin: 0.4rem 0 0;
    font-size: 0.82rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-400));
  }

  .liquid-brief-tone {
    margin: 0.4rem 0 0;
    font-size: 0.6rem;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .liquid-brief-sections {
    display: flex;
    flex-direction: column;
    gap: 0.95rem;
  }

  .liquid-brief-heading {
    margin: 0 0 0.35rem;
    font-size: 0.88rem;
    font-weight: 650;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-100));
  }

  .liquid-brief-heading :global(strong) {
    font-weight: 750;
    color: inherit;
  }

  .liquid-brief-title :global(strong),
  .liquid-brief-subtitle :global(strong) {
    font-weight: 750;
    color: inherit;
  }

  .liquid-brief-body {
    margin: 0;
    font-size: 0.82rem;
    line-height: 1.55;
    color: rgb(var(--color-surface-200));
    min-width: 0;
  }

  .liquid-brief-body :global(.markdown-content) {
    font-size: inherit;
    line-height: inherit;
    color: inherit;
  }

  .liquid-brief-body :global(.markdown-content > :first-child) {
    margin-top: 0;
  }

  .liquid-brief-body :global(.markdown-content > :last-child) {
    margin-bottom: 0;
  }

  .liquid-brief-body :global(.liquid-md-host) {
    margin: 0.75rem 0;
  }

  .liquid-brief-sources {
    margin-top: 1rem;
    padding-top: 0.75rem;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-500) 22%, transparent);
  }

  .liquid-brief-sources-label {
    margin: 0 0 0.45rem;
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }

  .liquid-brief-source-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }

  .liquid-brief-source-link,
  .liquid-brief-source-plain {
    display: flex;
    align-items: baseline;
    gap: 0.35rem;
    margin: 0;
    padding: 0;
    border: 0;
    background: transparent;
    color: rgb(var(--color-primary-300));
    font: inherit;
    text-align: left;
    text-decoration: none;
    cursor: pointer;
  }

  .liquid-brief-source-link:hover .liquid-brief-source-title,
  .liquid-brief-source-plain:hover .liquid-brief-source-title {
    text-decoration: underline;
  }

  .liquid-brief-source-plain {
    color: rgb(var(--color-surface-200));
  }

  .liquid-brief-source-index {
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--color-surface-500));
    flex-shrink: 0;
  }

  .liquid-brief-source-title {
    font-size: 0.75rem;
    font-weight: 600;
    line-height: 1.35;
  }

  .liquid-brief-source-quote {
    margin: 0.2rem 0 0 1.1rem;
    font-size: 0.7rem;
    font-style: italic;
    line-height: 1.4;
    color: rgb(var(--color-surface-400));
  }
</style>
