<script lang="ts">
  /** `prose` atom — narrative text. Wraps the existing markdown pipeline. */
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();

  const content = $derived(typeof node.props.markdown === "string" ? node.props.markdown : "");
</script>

<div class="liquid-prose">
  <MarkdownContent
    {content}
    titleByPath={ctx.titleByPath}
    openLinksInWeb={ctx.openLinksInWeb ?? false}
  />
</div>

<style>
  .liquid-prose {
    min-width: 0;
    max-width: 100%;
  }
</style>
