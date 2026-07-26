<script lang="ts">
  import { tick } from "svelte";
  import { ChevronLeft, CircleHelp, X } from "@lucide/svelte";
  import LiquidCardDetailSheet from "$lib/components/chat/LiquidCardDetailSheet.svelte";
  import MarkdownContent from "$lib/components/ui/MarkdownContent.svelte";
  import { chaptersByGroup, getGuideChapter } from "$lib/guide/catalog";
  import { loadGuideMarkdown } from "$lib/guide/loadGuide";
  import {
    GUIDE_HANDOFF_EVENT,
    closeGuide,
    parseGuideHref,
    readGuideHandoff,
    writeGuideHandoff,
  } from "$lib/guide/openGuide";
  import type { CardDetailPayload } from "$lib/markdown/liquidEmbeds";
  import { isTauri } from "$lib/window";

  interface OutlineItem {
    id: string;
    text: string;
    depth: number;
  }

  const groups = chaptersByGroup();

  let chapterId = $state(readGuideHandoff().chapterId);
  let pendingAnchor = $state<string | null>(readGuideHandoff().anchor ?? null);
  let outline = $state<OutlineItem[]>([]);
  let tocOpen = $state(true);
  let readerEl = $state<HTMLElement | null>(null);
  let markdownHost = $state<HTMLElement | null>(null);
  let cardDetailOpen = $state(false);
  let cardDetail = $state<CardDetailPayload | null>(null);

  const chapter = $derived(getGuideChapter(chapterId));
  const markdown = $derived(loadGuideMarkdown(chapterId) ?? "_Chapter missing._");

  function openCardDetail(detail: CardDetailPayload) {
    cardDetail = detail;
    cardDetailOpen = true;
  }

  function closeCardDetail() {
    cardDetailOpen = false;
    cardDetail = null;
  }

  function openChapter(nextId: string, anchor?: string | null) {
    const next = getGuideChapter(nextId);
    if (!next) return;
    const sameChapter = next.id === chapterId;
    chapterId = next.id;
    pendingAnchor = anchor?.trim() || null;
    closeCardDetail();
    writeGuideHandoff(next.id, pendingAnchor);
    if (sameChapter && pendingAnchor) {
      void scrollToPendingAnchor();
    }
  }

  function applyHandoff() {
    const handoff = readGuideHandoff();
    chapterId = handoff.chapterId;
    pendingAnchor = handoff.anchor ?? null;
  }

  function rebuildOutline() {
    const root = markdownHost?.querySelector(".markdown-content");
    if (!root) {
      outline = [];
      return;
    }
    const headings = root.querySelectorAll<HTMLElement>("h2[id], h3[id]");
    outline = [...headings].map((el) => ({
      id: el.id,
      text: (el.textContent ?? "").trim(),
      depth: el.tagName === "H3" ? 3 : 2,
    }));
  }

  async function scrollToPendingAnchor() {
    if (!pendingAnchor || !readerEl) return;
    await tick();
    const target =
      readerEl.querySelector<HTMLElement>(`#${CSS.escape(pendingAnchor)}`) ??
      readerEl.querySelector<HTMLElement>(
        `[data-heading-slug="${CSS.escape(pendingAnchor)}"]`,
      );
    if (target) {
      const accordionItem = target.closest(".liquid-accordion-item");
      const trigger = accordionItem?.querySelector<HTMLButtonElement>(
        ".liquid-accordion-trigger",
      );
      if (trigger?.getAttribute("aria-expanded") === "false") {
        trigger.click();
        await tick();
      }
      target.scrollIntoView({ block: "start", behavior: "smooth" });
    }
    pendingAnchor = null;
  }

  function onReaderClick(event: MouseEvent) {
    const anchor = (event.target as HTMLElement | null)?.closest("a");
    if (!anchor || !(anchor instanceof HTMLAnchorElement)) return;
    const href =
      anchor.getAttribute("data-guide-href")?.trim() ||
      anchor.getAttribute("href")?.trim() ||
      "";
    const guide = parseGuideHref(href);
    if (guide) {
      event.preventDefault();
      openChapter(guide.chapterId, guide.anchor);
      return;
    }
    if (href.startsWith("#") && href.length > 1) {
      event.preventDefault();
      pendingAnchor = href.slice(1);
      void scrollToPendingAnchor();
    }
  }

  $effect(() => {
    markdown;
    chapterId;
    void tick().then(async () => {
      rebuildOutline();
      if (pendingAnchor) {
        await scrollToPendingAnchor();
      } else if (readerEl) {
        readerEl.scrollTop = 0;
      }
    });
  });

  $effect(() => {
    const onHandoff = () => applyHandoff();
    const onStorage = (event: StorageEvent) => {
      if (event.key === "medousa.guide.chapter" || event.key === "medousa.guide.anchor") {
        applyHandoff();
      }
    };
    window.addEventListener(GUIDE_HANDOFF_EVENT, onHandoff);
    window.addEventListener("storage", onStorage);
    return () => {
      window.removeEventListener(GUIDE_HANDOFF_EVENT, onHandoff);
      window.removeEventListener("storage", onStorage);
    };
  });
</script>

<div class="guide-shell">
  <header class="guide-titlebar" data-tauri-drag-region={isTauri() ? true : undefined}>
    <div class="guide-titlebar-lead">
      <button
        type="button"
        class="guide-icon-btn"
        class:guide-icon-btn-active={tocOpen}
        aria-label={tocOpen ? "Hide table of contents" : "Show table of contents"}
        title="Table of contents"
        onclick={() => (tocOpen = !tocOpen)}
      >
        <ChevronLeft
          size={16}
          strokeWidth={1.85}
          class={tocOpen ? "" : "guide-toc-chevron-closed"}
        />
      </button>
      <CircleHelp size={15} strokeWidth={1.85} class="guide-title-mark" />
      <div class="min-w-0">
        <h1 class="guide-title">Operator's Guide</h1>
        {#if chapter}
          <p class="guide-subtitle">{chapter.title}</p>
        {/if}
      </div>
    </div>
    <button
      type="button"
      class="guide-icon-btn"
      aria-label="Close guide"
      title="Close"
      onclick={() => void closeGuide()}
    >
      <X size={16} strokeWidth={1.85} />
    </button>
  </header>

  <div class="guide-body" class:guide-body-toc-collapsed={!tocOpen}>
    <aside class="guide-toc" aria-label="Chapters">
      {#each groups as { group, chapters } (group.id)}
        <div class="guide-toc-group">
          <p class="guide-toc-group-label">{group.label}</p>
          <ul class="guide-toc-list">
            {#each chapters as entry (entry.id)}
              <li>
                <button
                  type="button"
                  class="guide-toc-item"
                  class:guide-toc-item-active={entry.id === chapterId}
                  onclick={() => openChapter(entry.id)}
                >
                  {entry.title}
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/each}
    </aside>

    <main
      bind:this={readerEl}
      class="guide-reader"
      onclick={onReaderClick}
    >
      <div bind:this={markdownHost} class="guide-reader-measure">
        <MarkdownContent
          content={markdown}
          liquidContext={{ onOpenCardDetail: openCardDetail }}
        />
      </div>
    </main>

    <LiquidCardDetailSheet
      open={cardDetailOpen}
      detail={cardDetail}
      onClose={closeCardDetail}
    />

    {#if outline.length > 0}
      <aside class="guide-outline" aria-label="On this page">
        <p class="guide-outline-label">On this page</p>
        <ul class="guide-outline-list">
          {#each outline as item (item.id)}
            <li>
              <button
                type="button"
                class="guide-outline-item"
                class:guide-outline-item-h3={item.depth === 3}
                onclick={() => {
                  pendingAnchor = item.id;
                  void scrollToPendingAnchor();
                }}
              >
                {item.text}
              </button>
            </li>
          {/each}
        </ul>
      </aside>
    {/if}
  </div>
</div>

<style>
  .guide-shell {
    display: flex;
    height: 100vh;
    width: 100vw;
    flex-direction: column;
    background: rgb(var(--color-surface-950));
    color: rgb(var(--color-surface-50));
  }

  .guide-titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    border-bottom: 1px solid rgb(var(--color-surface-600) / 0.28);
    padding: 0.55rem 0.75rem;
  }

  .guide-titlebar-lead {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.55rem;
  }

  .guide-title-mark {
    flex-shrink: 0;
    opacity: 0.7;
  }

  .guide-title {
    margin: 0;
    font-size: 0.8125rem;
    font-weight: 600;
    line-height: 1.2;
  }

  .guide-subtitle {
    margin: 0.1rem 0 0;
    font-size: 0.68rem;
    color: rgb(var(--color-surface-500));
  }

  .guide-icon-btn {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    width: 1.75rem;
    height: 1.75rem;
    border: 0;
    border-radius: 0.4rem;
    background: transparent;
    color: rgb(var(--color-surface-500));
    cursor: pointer;
  }

  .guide-icon-btn:hover,
  .guide-icon-btn-active {
    color: rgb(var(--color-surface-200));
    background: rgb(var(--color-surface-800) / 0.55);
  }

  .guide-icon-btn :global(.guide-toc-chevron-closed) {
    transform: rotate(180deg);
  }

  .guide-body {
    display: grid;
    min-height: 0;
    flex: 1;
    grid-template-columns: 13.5rem minmax(0, 1fr) 11rem;
  }

  .guide-body-toc-collapsed {
    grid-template-columns: 0 minmax(0, 1fr) 11rem;
  }

  .guide-body-toc-collapsed .guide-toc {
    overflow: hidden;
    border-right-width: 0;
    opacity: 0;
    pointer-events: none;
  }

  .guide-toc {
    min-height: 0;
    overflow-y: auto;
    border-right: 1px solid rgb(var(--color-surface-600) / 0.22);
    padding: 0.85rem 0.55rem 1.25rem;
    transition: opacity 150ms ease;
  }

  .guide-toc-group + .guide-toc-group {
    margin-top: 1rem;
  }

  .guide-toc-group-label {
    margin: 0 0 0.35rem 0.45rem;
    font-size: 0.62rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-600));
  }

  .guide-toc-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .guide-toc-item {
    display: block;
    width: 100%;
    border: 0;
    border-radius: 0.4rem;
    background: transparent;
    padding: 0.35rem 0.45rem;
    text-align: left;
    font-size: 0.78rem;
    color: rgb(var(--color-surface-400));
    cursor: pointer;
  }

  .guide-toc-item:hover {
    color: rgb(var(--color-surface-200));
    background: rgb(var(--color-surface-800) / 0.45);
  }

  .guide-toc-item-active {
    color: rgb(var(--color-surface-50));
    background: rgb(var(--color-surface-800) / 0.7);
  }

  .guide-reader {
    min-height: 0;
    overflow-y: auto;
    padding: 1.5rem 1.75rem 3rem;
  }

  .guide-reader-measure {
    max-width: 42rem;
    margin: 0 auto;
  }

  .guide-outline {
    min-height: 0;
    overflow-y: auto;
    border-left: 1px solid rgb(var(--color-surface-600) / 0.22);
    padding: 0.85rem 0.65rem 1.25rem;
  }

  .guide-outline-label {
    margin: 0 0 0.4rem 0.35rem;
    font-size: 0.62rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-600));
  }

  .guide-outline-list {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .guide-outline-item {
    display: block;
    width: 100%;
    border: 0;
    border-radius: 0.35rem;
    background: transparent;
    padding: 0.28rem 0.35rem;
    text-align: left;
    font-size: 0.7rem;
    line-height: 1.35;
    color: rgb(var(--color-surface-500));
    cursor: pointer;
  }

  .guide-outline-item-h3 {
    padding-left: 0.85rem;
  }

  .guide-outline-item:hover {
    color: rgb(var(--color-surface-200));
  }

  @media (max-width: 900px) {
    .guide-body,
    .guide-body-toc-collapsed {
      grid-template-columns: 12rem minmax(0, 1fr);
    }

    .guide-body-toc-collapsed {
      grid-template-columns: 0 minmax(0, 1fr);
    }

    .guide-outline {
      display: none;
    }
  }
</style>
