<script lang="ts">
  import { renderMarkdown } from "$lib/markdown";
  import { hydrateMarkdownContainer } from "$lib/markdown/hydrateMarkdownContainer";
  import { destroyLiquidEmbeds } from "$lib/markdown/hydrateLiquidEmbeds";
  import {
    getLiquidContext,
    type LiquidRenderContext,
  } from "$lib/liquid/render/context";
  import { openInBrowser, isHttpUrl } from "$lib/utils/openInBrowser";
  import { onDestroy } from "svelte";

  interface Props {
    content: string;
    titleByPath?: Map<string, string>;
    /** Open http(s) links in the Web surface instead of a new tab. */
    openLinksInWeb?: boolean;
    /** Optional override; defaults to inherited Liquid context when inside a scene. */
    liquidContext?: LiquidRenderContext;
  }

  let {
    content,
    titleByPath,
    openLinksInWeb = false,
    liquidContext,
  }: Props = $props();

  let container: HTMLDivElement | undefined = $state();

  const inherited = getLiquidContext();

  function resolveContext(): LiquidRenderContext {
    return {
      ...inherited,
      ...liquidContext,
      openLinksInWeb: liquidContext?.openLinksInWeb ?? openLinksInWeb ?? inherited.openLinksInWeb,
      titleByPath: titleByPath ?? liquidContext?.titleByPath ?? inherited.titleByPath,
      localImagePath:
        liquidContext?.localImagePath ?? inherited.localImagePath ?? null,
    };
  }

  const html = $derived.by(() => {
    const ctx = resolveContext();
    return renderMarkdown(content, {
      titleByPath: ctx.titleByPath,
      // Nested slide/report bodies need the same vault image pipeline as preview.
      resolveLocalImages: Boolean(ctx.localImagePath),
    });
  });

  function handleLinkClick(event: MouseEvent) {
    if (!openLinksInWeb) return;
    const target = (event.target as HTMLElement | null)?.closest("a");
    if (!target || !(target instanceof HTMLAnchorElement)) return;
    const href = target.getAttribute("href")?.trim();
    if (!href || !isHttpUrl(href)) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.button === 1) return;
    event.preventDefault();
    void openInBrowser(href, { openedBy: "user" });
  }

  function handleLinkKeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" && event.key !== " ") return;
    handleLinkClick(event as unknown as MouseEvent);
  }

  $effect(() => {
    html;
    if (!container) return;
    const ctx = resolveContext();
    void hydrateMarkdownContainer(container, {
      liquidContext: ctx,
      localImagePath: ctx.localImagePath ?? null,
      code: true,
      mermaid: true,
      liquid: true,
      localImages: Boolean(ctx.localImagePath),
    });
  });

  onDestroy(() => {
    if (container) destroyLiquidEmbeds(container);
  });
</script>

<!-- Link clicks delegate to Web surface; keyboard activation uses the same path. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  bind:this={container}
  class="markdown-content min-w-0 max-w-full"
  role="document"
  onclick={handleLinkClick}
  onkeydown={handleLinkKeydown}
>
  {@html html}
</div>
