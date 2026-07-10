<script lang="ts">
  import { renderMarkdown } from "$lib/markdown";
  import { hydrateCodeBlocks } from "$lib/markdown/codeBlocks";
  import { hydrateMermaid } from "$lib/markdown/mermaid";
  import {
    destroyLiquidEmbeds,
    hydrateLiquidEmbeds,
  } from "$lib/markdown/hydrateLiquidEmbeds";
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
  const html = $derived(renderMarkdown(content, titleByPath));

  function resolveContext(): LiquidRenderContext {
    return {
      ...inherited,
      ...liquidContext,
      openLinksInWeb: liquidContext?.openLinksInWeb ?? openLinksInWeb ?? inherited.openLinksInWeb,
      titleByPath: titleByPath ?? liquidContext?.titleByPath ?? inherited.titleByPath,
    };
  }

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

  $effect(() => {
    html;
    if (!container) return;
    destroyLiquidEmbeds(container);
    void hydrateCodeBlocks(container);
    void hydrateMermaid(container);
    hydrateLiquidEmbeds(container, resolveContext());
  });

  onDestroy(() => {
    if (container) destroyLiquidEmbeds(container);
  });
</script>

<div
  bind:this={container}
  class="markdown-content min-w-0 max-w-full text-sm leading-relaxed"
  onclick={handleLinkClick}
>
  {@html html}
</div>
