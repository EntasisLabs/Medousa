<script lang="ts">
  /** `prose` atom — narrative text. Wraps the existing markdown pipeline. */
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const content = $derived(typeof node.props.markdown === "string" ? node.props.markdown : "");
  /** Plain mode renders verbatim text (user/system turns), never parsed as markdown. */
  const plain = $derived(node.props.plain === true);
</script>

<div class="liquid-prose">
  {#if plain}
    <p class="liquid-prose-plain">{content}</p>
  {:else}
    <MarkdownContent
      {content}
      titleByPath={ctx.titleByPath}
      openLinksInWeb={ctx.openLinksInWeb ?? false}
    />
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
