<script lang="ts">
  /** `prose` atom — narrative text. Uses the parse renderer, not MarkdownContent. */
  import { renderMarkdown } from "$lib/markdown/render";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const content = $derived(typeof node.props.markdown === "string" ? node.props.markdown : "");
  const plain = $derived(node.props.plain === true);
  const html = $derived(
    plain
      ? ""
      : renderMarkdown(content, {
          titleByPath: ctx.titleByPath,
        }),
  );
</script>

<div class="liquid-prose">
  {#if plain}
    <p class="liquid-prose-plain">{content}</p>
  {:else}
    <div class="markdown-content min-w-0 max-w-full">{@html html}</div>
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
