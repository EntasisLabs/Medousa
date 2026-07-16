<script lang="ts">
  import { onDestroy, tick, untrack } from "svelte";
  import { renderMarkdownPreview, type MarkdownRenderOptions } from "$lib/markdown/render";
  import { hydrateMarkdownContainer } from "$lib/markdown/hydrateMarkdownContainer";
  import { destroyLiquidEmbeds } from "$lib/markdown/hydrateLiquidEmbeds";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultFind } from "$lib/stores/vaultFind.svelte";
  import {
    clearFindHighlights,
    renderedPlainText,
  } from "$lib/utils/vaultFindInNote";
  import { scrollToHeadingInContainer } from "$lib/utils/headingSlug";
  import { hasMedousaViewBlocks } from "$lib/utils/markdownView";
  import { resolveMedousaViews } from "$lib/utils/resolveMedousaViews";
  import { hasTransclusionBlocks, resolveTransclusions } from "$lib/utils/resolveTransclusion";
  import { stripFrontmatter } from "$lib/utils/vaultFrontmatter";
  import {
    bindVaultLongPress,
    handleVaultNoteContextMenuEvent,
  } from "$lib/utils/vaultContextMenuEvents";
  import { copyTextToClipboard } from "$lib/utils/vaultClipboard";

  interface Props {
    content: string;
    labelByPath: Map<string, string>;
    compact?: boolean;
    /** Allow Configure on medousa-view blocks (current note only). */
    configureViews?: boolean;
    onWikilink?: (target: string) => void;
    /** Split edit: jump source toward the clicked heading. */
    onHeadingClick?: (headingText: string) => void;
    /** Scroll container (the article) for split-pane sync. */
    scrollEl?: HTMLElement | null;
  }

  let {
    content,
    labelByPath,
    compact = false,
    configureViews = true,
    onWikilink,
    onHeadingClick,
    scrollEl = $bindable<HTMLElement | null>(null),
  }: Props = $props();

  const body = $derived(stripFrontmatter(content).content);
  const hasViewBlocks = $derived(hasMedousaViewBlocks(body));
  const hasEmbedBlocks = $derived(hasTransclusionBlocks(body));
  const needsAsyncResolve = $derived(hasViewBlocks || hasEmbedBlocks);

  let resolvedBody = $state("");
  let viewsResolving = $state(false);
  let resolveEpoch = 0;

  const renderOptions = $derived.by((): MarkdownRenderOptions => ({
    titleByPath: labelByPath,
    sourcePath: vault.selectedPath,
    knownPaths: new Set(vault.notes.map((note) => note.path)),
    interactiveTasks:
      !needsAsyncResolve && Boolean(vault.selectedPath) && !vault.proposalActive,
    resolveLocalImages: Boolean(vault.selectedPath),
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

    if (!needsAsyncResolve) {
      resolvedBody = source;
      viewsResolving = false;
      return;
    }

    const epoch = ++resolveEpoch;
    viewsResolving = true;
    resolvedBody = "";
    void (async () => {
      let next = source;
      if (hasMedousaViewBlocks(next)) {
        next = await resolveMedousaViews(next, {
          sourcePath: path,
          notes: vault.notes,
          selectedPath: path,
          selectedContent: fullContent,
          labelByPath,
        });
      }
      if (hasTransclusionBlocks(next)) {
        next = await resolveTransclusions(next, {
          sourcePath: path,
          notes: vault.notes,
          selectedPath: path,
          selectedContent: fullContent,
          labelByPath,
        });
      }
      if (epoch !== resolveEpoch) return;
      resolvedBody = next;
      viewsResolving = false;
    })();
  });

  $effect(() => {
    previewHtml;
    if (!scrollEl) return;
    const titles = untrack(() => labelByPath);
    const imagePath = untrack(() => vault.selectedPath);
    void hydrateMarkdownContainer(scrollEl, {
      liquidContext: {
        titleByPath: titles,
        openLinksInWeb: false,
      },
      localImagePath: imagePath,
      code: true,
      mermaid: true,
      liquid: true,
      localImages: true,
    });
  });

  onDestroy(() => {
    if (scrollEl) destroyLiquidEmbeds(scrollEl);
  });

  $effect(() => {
    vault.headingScrollRequest;
    const heading = vault.pendingHeadingScroll;
    if (!heading || !scrollEl) return;
    void tick().then(() => {
      if (scrollEl) {
        scrollToHeadingInContainer(scrollEl, heading);
      }
    });
  });

  $effect(() => {
    vaultFind.registerPreview(scrollEl ?? null);
    return () => vaultFind.registerPreview(null);
  });

  $effect(() => {
    if (!scrollEl) return;
    if (!vaultFind.open || vault.editorMode === "edit") {
      clearFindHighlights(scrollEl);
    }
  });

  $effect(() => {
    previewHtml;
    if (!scrollEl || !vaultFind.open || vault.editorMode === "edit") return;
    void tick().then(() => {
      if (!scrollEl || !vaultFind.open || vault.editorMode === "edit") return;
      vaultFind.setSourceText(renderedPlainText(scrollEl));
    });
  });

  function scrollFromLink(raw: string | null | undefined) {
    if (!raw || !scrollEl) return;
    scrollToHeadingInContainer(scrollEl, raw.startsWith("#") ? raw.slice(1) : raw);
  }

  function handleChange(event: Event) {
    const target = event.target;
    if (!(target instanceof HTMLInputElement)) return;
    if (target.type !== "checkbox" || !target.classList.contains("vault-preview-task")) return;
    const raw = target.getAttribute("data-vault-task");
    if (raw == null) return;
    const index = Number(raw);
    if (!Number.isFinite(index)) return;
    vault.togglePreviewTask(index, target.checked);
  }

  function handleClick(event: MouseEvent) {
    const configureChart = (event.target as HTMLElement).closest(
      ".liquid-chart-configure",
    );
    if (configureChart) {
      const shell = configureChart.closest("[data-edit-chart-index]");
      event.preventDefault();
      if (!configureViews || content !== vault.content || !shell) return;
      const raw = shell.getAttribute("data-edit-chart-index");
      const index = raw == null ? NaN : Number(raw);
      if (Number.isFinite(index)) {
        vault.openChartBridgeEdit(index);
      }
      return;
    }

    const configureView = (event.target as HTMLElement).closest(
      "[data-edit-view-index]",
    );
    if (configureView) {
      event.preventDefault();
      if (!configureViews || content !== vault.content) return;
      const raw = configureView.getAttribute("data-edit-view-index");
      const index = raw == null ? NaN : Number(raw);
      if (Number.isFinite(index)) {
        vault.openViewBridgeEdit(index);
      }
      return;
    }

    const openSource = (event.target as HTMLElement).closest("[data-open-vault-note]");
    if (openSource) {
      event.preventDefault();
      const path = openSource.getAttribute("data-open-vault-note");
      if (path) void vault.openNote(path);
      return;
    }

    const copyCsv = (event.target as HTMLElement).closest("[data-copy-view-csv]");
    if (copyCsv) {
      event.preventDefault();
      const payload =
        copyCsv.getAttribute("data-view-csv") ??
        copyCsv.getAttribute("data-copy-view-csv");
      if (payload) {
        try {
          void copyTextToClipboard(decodeURIComponent(payload));
        } catch {
          // ignore malformed payloads
        }
      }
      return;
    }

    const wikilink = (event.target as HTMLElement).closest("[data-wikilink]");
    if (wikilink && onWikilink) {
      event.preventDefault();
      const raw = wikilink.getAttribute("data-wikilink");
      if (raw) onWikilink(raw);
      return;
    }

    if (onHeadingClick) {
      const heading = (event.target as HTMLElement).closest("h1, h2, h3, h4, h5, h6");
      if (heading && scrollEl?.contains(heading)) {
        const text = (heading.textContent ?? "").trim();
        if (text) {
          onHeadingClick(text);
          return;
        }
      }
    }

    const tocLink = (event.target as HTMLElement).closest("[data-heading-link]");
    if (tocLink) {
      event.preventDefault();
      scrollFromLink(tocLink.getAttribute("data-heading-link"));
      return;
    }

    const hashLink = (event.target as HTMLElement).closest('a[href^="#"]');
    if (hashLink && scrollEl?.contains(hashLink)) {
      const href = hashLink.getAttribute("href");
      if (href && href.length > 1) {
        event.preventDefault();
        scrollFromLink(href);
      }
    }
  }

  function handleContextMenu(event: MouseEvent) {
    const path = vault.selectedPath;
    if (!path) return;
    const sel = typeof window !== "undefined" ? window.getSelection() : null;
    let selection: { text: string } | null = null;
    if (
      sel &&
      !sel.isCollapsed &&
      sel.rangeCount > 0 &&
      scrollEl &&
      (scrollEl.contains(sel.anchorNode) || scrollEl.contains(sel.focusNode))
    ) {
      const text = sel.toString();
      if (text.trim()) selection = { text };
    }
    handleVaultNoteContextMenuEvent(path, event, selection);
  }

  function handleKeydown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
      event.preventDefault();
      event.stopPropagation();
      if (scrollEl) {
        vaultFind.setSourceText(renderedPlainText(scrollEl));
      }
      vaultFind.openFind();
      return;
    }
    if (event.key !== "Enter" && event.key !== " ") return;
    const configureChart = (event.target as HTMLElement).closest(
      ".liquid-chart-configure",
    );
    if (configureChart) {
      const shell = configureChart.closest("[data-edit-chart-index]");
      event.preventDefault();
      if (!configureViews || content !== vault.content || !shell) return;
      const raw = shell.getAttribute("data-edit-chart-index");
      const index = raw == null ? NaN : Number(raw);
      if (Number.isFinite(index)) {
        vault.openChartBridgeEdit(index);
      }
      return;
    }
    const configureView = (event.target as HTMLElement).closest(
      "[data-edit-view-index]",
    );
    if (configureView) {
      event.preventDefault();
      if (!configureViews || content !== vault.content) return;
      const raw = configureView.getAttribute("data-edit-view-index");
      const index = raw == null ? NaN : Number(raw);
      if (Number.isFinite(index)) {
        vault.openViewBridgeEdit(index);
      }
      return;
    }
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
  bind:this={scrollEl}
  class="markdown-content vault-markdown-preview min-w-0 max-w-full flex-1 overflow-x-auto overflow-y-auto text-sm {compact
    ? 'px-4 py-3'
    : 'px-5 py-4'}"
  onclick={handleClick}
  onchange={handleChange}
  onkeydown={handleKeydown}
  oncontextmenu={handleContextMenu}
  use:bindVaultLongPress={() => vault.selectedPath}
>
  {#if viewsResolving && needsAsyncResolve && !previewHtml}
    <p class="workshop-faint text-sm">Loading preview…</p>
  {:else if previewHtml}
    {@html previewHtml}
  {:else}
    <p class="workshop-faint text-sm">Nothing to preview yet.</p>
  {/if}
</article>
