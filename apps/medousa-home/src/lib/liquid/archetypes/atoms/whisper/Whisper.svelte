<script lang="ts">
  import "$lib/styles/markdown-content.postcss";
  /** `whisper` atom — a muted stage-direction line above the main voice. */
  import { renderMarkdown } from "$lib/markdown/render";
  import { getLiquidContext } from "$lib/liquid/render/context";
  import type { ArchetypeProps } from "$lib/liquid/render/types";

  let { node }: ArchetypeProps = $props();
  const ctx = getLiquidContext();
  const text = $derived(typeof node.props.text === "string" ? node.props.text : "");
  const html = $derived(
    renderMarkdown(text, {
      titleByPath: ctx.titleByPath,
    }),
  );
</script>

{#if text}
  <div class="liquid-whisper">
    <div class="markdown-content min-w-0 max-w-full">{@html html}</div>
  </div>
{/if}

<style>
  .liquid-whisper {
    margin: 0;
    font-size: 0.75rem;
    font-style: italic;
    color: color-mix(in srgb, rgb(var(--color-surface-300)) 80%, transparent);
  }

  .liquid-whisper :global(.markdown-content) {
    font-size: inherit;
    font-style: inherit;
    color: inherit;
    line-height: 1.4;
  }

  .liquid-whisper :global(.markdown-content p) {
    margin: 0;
  }
</style>
