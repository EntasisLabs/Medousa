<script lang="ts">
  import { onDestroy } from "svelte";

  import {
    destroyMarkdownContainer,
    hydrateMarkdownContainer,
  } from "$lib/markdown/hydrateMarkdownContainer";
  import type { LiquidRenderContext } from "$lib/liquid/render/context";

  interface Props {
    html: string;
    liquidContext: LiquidRenderContext;
  }

  let { html, liquidContext }: Props = $props();
  let container: HTMLDivElement | undefined = $state();
  let hydratedHtml: string | null = null;

  $effect(() => {
    if (!container || hydratedHtml === html) return;
    hydratedHtml = html;
    void hydrateMarkdownContainer(container, {
      liquidContext,
      localImagePath: liquidContext.localImagePath ?? null,
      code: true,
      mermaid: true,
      liquid: true,
      localImages: Boolean(liquidContext.localImagePath),
    });
  });

  onDestroy(() => {
    if (!container) return;
    void destroyMarkdownContainer(container);
  });
</script>

<div bind:this={container} data-stable-markdown-block="">{@html html}</div>
