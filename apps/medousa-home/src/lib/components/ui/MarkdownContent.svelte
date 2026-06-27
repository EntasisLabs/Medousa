<script lang="ts">
  import { renderMarkdown } from "$lib/markdown";
  import { hydrateCodeBlocks } from "$lib/markdown/codeBlocks";
  import { hydrateMermaid } from "$lib/markdown/mermaid";
  import { openInBrowser, isHttpUrl } from "$lib/utils/openInBrowser";

  interface Props {
    content: string;
    titleByPath?: Map<string, string>;
    /** Open http(s) links in the Web surface instead of a new tab. */
    openLinksInWeb?: boolean;
  }

  let { content, titleByPath, openLinksInWeb = false }: Props = $props();

  let container: HTMLDivElement | undefined = $state();

  const html = $derived(renderMarkdown(content, titleByPath));

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
    void hydrateCodeBlocks(container);
    void hydrateMermaid(container);
  });
</script>

<div
  bind:this={container}
  class="markdown-content min-w-0 max-w-full text-sm leading-relaxed"
  onclick={handleLinkClick}
>
  {@html html}
</div>
