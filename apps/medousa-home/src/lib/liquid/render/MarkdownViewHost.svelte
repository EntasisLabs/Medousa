<script lang="ts">
  /**
   * Nested markdown inside Liquid organisms. Hosts inject `markdownView`
   * (MarkdownContent) so archetypes never import the hydrate owner.
   */
  import { renderMarkdown } from "$lib/markdown/render";
  import { getLiquidContext } from "./context";
  import { getMarkdownViewComponent } from "$lib/components/ui/markdownView";

  interface Props {
    content: string;
    streaming?: boolean;
  }

  let { content, streaming = false }: Props = $props();
  const ctx = getLiquidContext();
  const View = $derived(ctx.markdownView ?? getMarkdownViewComponent());
  const html = $derived(
    View
      ? ""
      : renderMarkdown(content, {
          titleByPath: ctx.titleByPath,
          resolveLocalImages: Boolean(ctx.localImagePath),
        }),
  );
</script>

{#if View}
  <View
    {content}
    titleByPath={ctx.titleByPath}
    openLinksInWeb={ctx.openLinksInWeb ?? false}
    {streaming}
  />
{:else}
  <div class="markdown-content min-w-0 max-w-full">{@html html}</div>
{/if}
