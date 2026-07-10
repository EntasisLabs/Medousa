<script lang="ts">
  /**
   * `cite` molecule — curated quote + link from tool-sourced content
   * (web search, docs, etc.). Not a tool receipt — substance in the answer.
   */
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import { openInBrowser, isHttpUrl } from "$lib/utils/openInBrowser";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const title = $derived(typeof node.props.title === "string" ? node.props.title : "");
  const url = $derived(typeof node.props.url === "string" ? node.props.url.trim() : "");
  const quote = $derived(typeof node.props.quote === "string" ? node.props.quote : "");
  const source = $derived(typeof node.props.source === "string" ? node.props.source : "");
  const hasLink = $derived(Boolean(url && isHttpUrl(url)));

  function onLinkClick(event: MouseEvent) {
    if (!hasLink) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.button === 1) return;
    if (ctx.openLinksInWeb) {
      event.preventDefault();
      void openInBrowser(url, { openedBy: "user" });
    }
  }
</script>

<figure class="liquid-cite">
  {#if quote}
    <blockquote class="liquid-cite-quote">{quote}</blockquote>
  {/if}
  <figcaption class="liquid-cite-meta">
    {#if hasLink}
      <a
        class="liquid-cite-title"
        href={url}
        target="_blank"
        rel="noopener noreferrer"
        onclick={onLinkClick}
      >
        {title || url}
      </a>
    {:else if title}
      <span class="liquid-cite-title liquid-cite-title-plain">{title}</span>
    {/if}
    {#if source}
      <span class="liquid-cite-source">{source}</span>
    {/if}
  </figcaption>
</figure>

<style>
  .liquid-cite {
    margin: 0;
    padding: 0.75rem 0.9rem;
    border-radius: 0.75rem;
    border: 1px solid color-mix(in srgb, var(--color-surface-500) 28%, transparent);
    background: color-mix(in srgb, var(--color-surface-900) 48%, transparent);
    box-shadow: inset 0 1px 0 color-mix(in srgb, var(--color-surface-50) 4%, transparent);
  }

  .liquid-cite-quote {
    margin: 0;
    padding: 0;
    border: 0;
    font-size: 0.84rem;
    line-height: 1.55;
    color: rgb(var(--color-surface-100));
    font-style: italic;
  }

  .liquid-cite-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.35rem 0.65rem;
    margin-top: 0.55rem;
  }

  .liquid-cite-title {
    font-size: 0.72rem;
    font-weight: 600;
    color: rgb(var(--color-primary-300));
    text-decoration: none;
  }

  .liquid-cite-title:hover {
    text-decoration: underline;
  }

  .liquid-cite-title-plain {
    color: rgb(var(--color-surface-200));
  }

  .liquid-cite-source {
    font-size: 0.65rem;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-500));
  }
</style>
