<script lang="ts">
  import {
    createMarkdownRenderSession,
    renderMarkdown,
    type MarkdownRenderSession,
  } from "$lib/markdown/render";
  import { StreamingMarkdownBlocks } from "$lib/markdown/streamingBlocks";
  import { hydrateMarkdownContainer, destroyMarkdownContainer } from "$lib/markdown/hydrateMarkdownContainer";
  import {
    getLiquidContext,
    type LiquidRenderContext,
  } from "$lib/liquid/render/context";
  import { openInBrowser, isHttpUrl } from "$lib/utils/openInBrowser";
  import { onDestroy, untrack } from "svelte";
  import HydratedMarkdownBlock from "$lib/components/ui/HydratedMarkdownBlock.svelte";
  import { getMarkdownViewComponent } from "$lib/components/ui/markdownView";

  interface Props {
    content: string;
    titleByPath?: Map<string, string>;
    /** Open http(s) links in the Web surface instead of a new tab. */
    openLinksInWeb?: boolean;
    /** Optional override; defaults to inherited Liquid context when inside a scene. */
    liquidContext?: LiquidRenderContext;
    /** Retain completed blocks while only the final Markdown token is changing. */
    streaming?: boolean;
  }

  let {
    content,
    titleByPath,
    openLinksInWeb = false,
    liquidContext,
    streaming = false,
  }: Props = $props();

  let container: HTMLDivElement | undefined = $state();

  const inherited = getLiquidContext();
  let stableMode = $state(untrack(() => streaming));
  let stableBlocks = $state<{ id: number; html: string }[]>([]);
  let tailHtml = $state("");
  let nextBlockId = 0;
  const streamingBlocks = new StreamingMarkdownBlocks();
  let renderSession: MarkdownRenderSession = createStreamingRenderSession();

  function resolveContext(): LiquidRenderContext {
    return {
      ...inherited,
      ...liquidContext,
      openLinksInWeb: liquidContext?.openLinksInWeb ?? openLinksInWeb ?? inherited.openLinksInWeb,
      titleByPath: titleByPath ?? liquidContext?.titleByPath ?? inherited.titleByPath,
      localImagePath:
        liquidContext?.localImagePath ?? inherited.localImagePath ?? null,
      markdownView:
        liquidContext?.markdownView ??
        inherited.markdownView ??
        getMarkdownViewComponent() ??
        undefined,
    };
  }

  function createStreamingRenderSession(): MarkdownRenderSession {
    const ctx = resolveContext();
    return createMarkdownRenderSession({
      titleByPath: ctx.titleByPath,
      resolveLocalImages: Boolean(ctx.localImagePath),
    });
  }

  const html = $derived.by(() => {
    if (stableMode) return "";
    const ctx = resolveContext();
    return renderMarkdown(content, {
      titleByPath: ctx.titleByPath,
      // Nested slide/report bodies need the same vault image pipeline as preview.
      resolveLocalImages: Boolean(ctx.localImagePath),
    });
  });

  $effect(() => {
    const source = content;
    const terminal = !streaming;
    if (streaming) stableMode = true;
    if (!stableMode) return;

    const update = streamingBlocks.update(source, terminal);
    if (update.reset) {
      stableBlocks = [];
      nextBlockId = 0;
      renderSession = createStreamingRenderSession();
    }
    if (update.completed.length > 0) {
      stableBlocks = [
        ...stableBlocks,
        ...update.completed.map((block) => ({
          id: nextBlockId++,
          html: renderSession.renderStable(block),
        })),
      ];
    }
    tailHtml = renderSession.renderTail(update.tail);
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
    if (stableMode) return;
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
    if (container) {
      void destroyMarkdownContainer(container);
    }
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
  {#if stableMode}
    {@const ctx = resolveContext()}
    {#each stableBlocks as block (block.id)}
      <HydratedMarkdownBlock html={block.html} liquidContext={ctx} />
    {/each}
    {#if tailHtml}
      <div data-streaming-markdown-tail="">{@html tailHtml}</div>
    {/if}
  {:else}
    {@html html}
  {/if}
</div>
