<script lang="ts">
  import "$lib/styles/markdown-content.postcss";
  /** `prose` atom — use the surface's Markdown view when one is provided. */
  import { renderMarkdown } from "$lib/markdown/render";
  import { codeBlockControls } from "$lib/markdown/codeBlocks";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const content = $derived(typeof node.props.markdown === "string" ? node.props.markdown : "");
  const plain = $derived(node.props.plain === true);
  const streaming = $derived(node.props.streaming === true);
  const MarkdownView = ctx.markdownView;
  const html = $derived(
    plain || MarkdownView
      ? ""
      : renderMarkdown(content, {
          titleByPath: ctx.titleByPath,
        }),
  );
</script>

<div class="liquid-prose">
  {#if plain}
    <p class="liquid-prose-plain">{content}</p>
  {:else if MarkdownView}
    <MarkdownView {content} {streaming} titleByPath={ctx.titleByPath} openLinksInWeb={ctx.openLinksInWeb} />
  {:else}
    <div class="markdown-content min-w-0 max-w-full" use:codeBlockControls>{@html html}</div>
  {/if}
</div>

<style>
  .liquid-prose {
    min-width: 0;
    max-width: 100%;
  }

  .liquid-prose-plain {
    margin: 0;
    white-space: pre-wrap;
    font-size: 0.875rem;
    line-height: 1.625;
  }
</style>
