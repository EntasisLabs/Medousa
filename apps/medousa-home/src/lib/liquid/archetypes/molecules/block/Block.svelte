<script lang="ts">
  /** `block` molecule — styled prose container (font / size / align / spacing + optional id). */
  import Slot from "$lib/liquid/render/Slot.svelte";
  import type { ArchetypeProps } from "$lib/liquid/render/types";
  import { styledBlockCssVars, type LiquidBlockProps } from "$lib/markdown/styledBlock";

  let { node }: ArchetypeProps = $props();

  const content = $derived(node.slots?.content ?? []);
  const styleVars = $derived.by(() => {
    const props: LiquidBlockProps = {
      body: "",
      id: typeof node.props.id === "string" ? node.props.id : undefined,
      font: typeof node.props.font === "string" ? (node.props.font as LiquidBlockProps["font"]) : undefined,
      size: typeof node.props.size === "string" ? node.props.size : undefined,
      align: typeof node.props.align === "string" ? (node.props.align as LiquidBlockProps["align"]) : undefined,
      spacing: typeof node.props.spacing === "string" ? node.props.spacing : undefined,
    };
    return styledBlockCssVars(props);
  });
  const blockId = $derived(typeof node.props.id === "string" ? node.props.id.trim() : "");
</script>

<div
  class="liquid-styled-block"
  style={Object.entries(styleVars)
    .map(([k, v]) => `${k}: ${v}`)
    .join("; ")}
  data-block-id={blockId || undefined}
  id={blockId ? `^${blockId}` : undefined}
>
  {#if content.length}
    <div class="liquid-styled-block__body">
      <Slot nodes={content} />
    </div>
  {/if}
</div>
