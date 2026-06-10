<script lang="ts">
  import { renderMarkdownPreview } from "$lib/markdown";
  import { hydrateCodeBlocks } from "$lib/markdown/codeBlocks";
  import { hydrateMermaid } from "$lib/markdown/mermaid";
  import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";

  interface Props {
    content: string;
    labelByPath: Map<string, string>;
    compact?: boolean;
    onWikilink?: (target: string) => void;
  }

  let { content, labelByPath, compact = false, onWikilink }: Props = $props();

  const body = $derived(stripFrontmatter(content).content);
  const previewHtml = $derived(
    body ? renderMarkdownPreview(body, labelByPath) : "",
  );

  let container: HTMLElement | undefined = $state();

  $effect(() => {
    previewHtml;
    if (!container) return;
    void hydrateCodeBlocks(container);
    void hydrateMermaid(container);
  });

  function handleClick(event: MouseEvent) {
    if (!onWikilink) return;
    const target = (event.target as HTMLElement).closest("[data-wikilink]");
    if (!target) return;
    event.preventDefault();
    const raw = target.getAttribute("data-wikilink");
    if (raw) onWikilink(raw);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<article
  bind:this={container}
  class="markdown-content vault-markdown-preview min-w-0 max-w-full flex-1 overflow-x-hidden overflow-y-auto text-sm {compact
    ? 'px-4 py-3'
    : 'px-5 py-4'}"
  onclick={handleClick}
>
  {#if previewHtml}
    {@html previewHtml}
  {:else}
    <p class="workshop-faint text-sm">Nothing to preview yet.</p>
  {/if}
</article>
