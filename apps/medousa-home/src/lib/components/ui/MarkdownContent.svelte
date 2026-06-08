<script lang="ts">
  import { renderMarkdown } from "$lib/markdown";
  import { hydrateMermaid } from "$lib/markdown/mermaid";

  interface Props {
    content: string;
    titleByPath?: Map<string, string>;
  }

  let { content, titleByPath }: Props = $props();

  let container: HTMLDivElement | undefined = $state();

  const html = $derived(renderMarkdown(content, titleByPath));

  $effect(() => {
    html;
    if (!container) return;
    void hydrateMermaid(container);
  });
</script>

<div bind:this={container} class="markdown-content text-sm leading-relaxed">
  {@html html}
</div>
