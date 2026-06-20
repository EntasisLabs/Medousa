<script lang="ts">
  import { tick } from "svelte";
  import { renderMarkdownPreview, type MarkdownRenderOptions } from "$lib/markdown/render";
  import { hydrateCodeBlocks } from "$lib/markdown/codeBlocks";
  import { hydrateMermaid } from "$lib/markdown/mermaid";
  import { vault } from "$lib/stores/vault.svelte";
  import { scrollToHeadingInContainer } from "$lib/utils/headingSlug";
  import { hasMedousaViewBlocks } from "$lib/utils/markdownView";
  import { resolveMedousaViews } from "$lib/utils/resolveMedousaViews";
  import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";

  interface Props {
    content: string;
    labelByPath: Map<string, string>;
    compact?: boolean;
    onWikilink?: (target: string) => void;
  }

  let { content, labelByPath, compact = false, onWikilink }: Props = $props();

  const body = $derived(stripFrontmatter(content).content);
  const hasViewBlocks = $derived(hasMedousaViewBlocks(body));

  let resolvedBody = $state("");
  let viewsResolving = $state(false);
  let resolveEpoch = 0;

  const renderOptions = $derived.by((): MarkdownRenderOptions => ({
    titleByPath: labelByPath,
    sourcePath: vault.selectedPath,
    knownPaths: new Set(vault.notes.map((note) => note.path)),
  }));

  const previewHtml = $derived(
    resolvedBody ? renderMarkdownPreview(resolvedBody, renderOptions) : "",
  );

  $effect(() => {
    const source = body;
    const path = vault.selectedPath;
    const fullContent = content;
    vault.contentRevision;
    labelByPath;

    if (!hasMedousaViewBlocks(source)) {
      resolvedBody = source;
      viewsResolving = false;
      return;
    }

    const epoch = ++resolveEpoch;
    viewsResolving = true;
    resolvedBody = "";
    void resolveMedousaViews(source, {
      sourcePath: path,
      notes: vault.notes,
      selectedPath: path,
      selectedContent: fullContent,
      labelByPath,
    }).then((next) => {
      if (epoch !== resolveEpoch) return;
      resolvedBody = next;
      viewsResolving = false;
    });
  });

  let container: HTMLElement | undefined = $state();

  $effect(() => {
    previewHtml;
    if (!container) return;
    void hydrateCodeBlocks(container);
    void hydrateMermaid(container);
  });

  $effect(() => {
    vault.headingScrollRequest;
    const heading = vault.pendingHeadingScroll;
    if (!heading || !container) return;
    void tick().then(() => {
      if (container) {
        scrollToHeadingInContainer(container, heading);
      }
    });
  });

  function scrollFromLink(raw: string | null | undefined) {
    if (!raw || !container) return;
    scrollToHeadingInContainer(container, raw.startsWith("#") ? raw.slice(1) : raw);
  }

  function handleClick(event: MouseEvent) {
    const openSource = (event.target as HTMLElement).closest("[data-open-vault-note]");
    if (openSource) {
      event.preventDefault();
      const path = openSource.getAttribute("data-open-vault-note");
      if (path) void vault.openNote(path);
      return;
    }

    const wikilink = (event.target as HTMLElement).closest("[data-wikilink]");
    if (wikilink && onWikilink) {
      event.preventDefault();
      const raw = wikilink.getAttribute("data-wikilink");
      if (raw) onWikilink(raw);
      return;
    }

    const tocLink = (event.target as HTMLElement).closest("[data-heading-link]");
    if (tocLink) {
      event.preventDefault();
      scrollFromLink(tocLink.getAttribute("data-heading-link"));
      return;
    }

    const hashLink = (event.target as HTMLElement).closest('a[href^="#"]');
    if (hashLink && container?.contains(hashLink)) {
      const href = hashLink.getAttribute("href");
      if (href && href.length > 1) {
        event.preventDefault();
        scrollFromLink(href);
      }
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" && event.key !== " ") return;
    const openSource = (event.target as HTMLElement).closest("[data-open-vault-note]");
    if (openSource) {
      event.preventDefault();
      const path = openSource.getAttribute("data-open-vault-note");
      if (path) void vault.openNote(path);
      return;
    }
    const wikilink = (event.target as HTMLElement).closest("[data-wikilink]");
    if (!wikilink || !onWikilink) return;
    event.preventDefault();
    const raw = wikilink.getAttribute("data-wikilink");
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
  onkeydown={handleKeydown}
>
  {#if viewsResolving && hasViewBlocks && !previewHtml}
    <p class="workshop-faint text-sm">Loading query views…</p>
  {:else if previewHtml}
    {@html previewHtml}
  {:else}
    <p class="workshop-faint text-sm">Nothing to preview yet.</p>
  {/if}
</article>
