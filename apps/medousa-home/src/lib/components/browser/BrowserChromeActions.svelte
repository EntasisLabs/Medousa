<script lang="ts">
  import {
    BookmarkPlus,
    ChevronDown,
    Copy,
    Ellipsis,
    ExternalLink,
    RefreshCw,
    RotateCcw,
    Search,
    Star,
    Undo2,
  } from "@lucide/svelte";
  import BrowserPopover from "$lib/components/browser/BrowserPopover.svelte";
  import BrowserSavedSheet from "$lib/components/browser/BrowserSavedSheet.svelte";
  import { browser } from "$lib/stores/browser.svelte";
  import { browserBookmarks } from "$lib/stores/browserBookmarks.svelte";
  import { humanBrowserForWindow } from "$lib/stores/humanBrowserSurface";

  const humanBrowser = $derived(humanBrowserForWindow());
  import { normalizeBrowserUrl } from "$lib/utils/browserUrl";
  import { copyBrowserUrl, openUrlInDefaultBrowser } from "$lib/utils/browserActions";
  import { toast } from "$lib/stores/toast.svelte";
  import {
    openSavedVaultNote,
    savePageToLibrary,
    showBookmarkFeedback,
    showSaveFeedback,
    togglePageBookmark,
  } from "$lib/utils/saveBrowserPage";

  interface Props {
    mobile?: boolean;
    onMobileToast?: (message: string, actionLabel?: string, onAction?: () => void) => void;
    /** Mobile overflow menu — reload current page. */
    onReload?: () => void | Promise<void>;
  }

  let { mobile = false, onMobileToast, onReload }: Props = $props();

  let saving = $state(false);
  let savedOpen = $state(false);
  let menuOpen = $state(false);
  let menuAnchorEl = $state<HTMLButtonElement | null>(null);
  let savedAnchorEl = $state<HTMLButtonElement | null>(null);
  let menuAnchorRect = $state<DOMRect | null>(null);
  let savedAnchorRect = $state<DOMRect | null>(null);

  const pageUrl = $derived(humanBrowser.activeUrl);
  const pageTitle = $derived(humanBrowser.activeTab?.title ?? "");
  const canAct = $derived(Boolean(pageUrl && pageUrl !== "about:blank"));
  const starred = $derived.by(() => {
    if (!canAct) return false;
    const norm = normalizeBrowserUrl(pageUrl);
    return browserBookmarks.bookmarks.some(
      (entry) => normalizeBrowserUrl(entry.url) === norm,
    );
  });
  const showHandBack = $derived(
    browser.control === "user" && Boolean(browser.scopedSessionId),
  );

  function refreshMenuAnchor() {
    menuAnchorRect = menuAnchorEl?.getBoundingClientRect() ?? null;
  }

  function closeMenu() {
    menuOpen = false;
  }

  function toggleMenu(event: MouseEvent) {
    event.stopPropagation();
    if (!menuOpen) {
      refreshMenuAnchor();
    }
    menuOpen = !menuOpen;
    if (menuOpen) {
      savedOpen = false;
    }
  }

  async function handleToggleStar(event?: MouseEvent) {
    event?.stopPropagation();
    if (!canAct) return;
    const result = await togglePageBookmark(pageUrl, pageTitle);
    if (mobile && onMobileToast) {
      onMobileToast(result === "added" ? "Bookmarked" : "Bookmark removed");
      closeMenu();
    } else {
      showBookmarkFeedback(result);
    }
  }

  async function handleSaveToLibrary(event?: MouseEvent) {
    event?.stopPropagation();
    if (!canAct || saving) return;
    saving = true;
    try {
      const result = await savePageToLibrary({
        url: pageUrl,
        title: pageTitle,
        openNote: false,
      });
      if (mobile && onMobileToast) {
        if (result.status === "error") {
          onMobileToast(result.message);
        } else if (result.status === "already_saved") {
          onMobileToast("Already saved in Library", "Open note", () => {
            openSavedVaultNote(result.path);
          });
        } else {
          onMobileToast("Saved to Library", "Open note", () => {
            openSavedVaultNote(result.path);
          });
        }
        closeMenu();
      } else {
        showSaveFeedback(result);
      }
    } finally {
      saving = false;
    }
  }

  function handleSavedToggle(event: MouseEvent) {
    event.stopPropagation();
    if (!savedOpen) {
      savedAnchorRect = savedAnchorEl?.getBoundingClientRect() ?? null;
    }
    savedOpen = !savedOpen;
    menuOpen = false;
  }

  function openSavedFromMenu() {
    closeMenu();
    savedAnchorRect = menuAnchorEl?.getBoundingClientRect() ?? null;
    savedOpen = true;
  }

  async function handleReloadFromMenu() {
    closeMenu();
    await onReload?.();
  }

  async function handleCopyUrl() {
    if (!canAct) return;
    const ok = await copyBrowserUrl(pageUrl);
    closeMenu();
    if (mobile && onMobileToast) {
      onMobileToast(ok ? "Link copied" : "Could not copy link");
    } else if (ok) {
      toast.show("Link copied");
    }
  }

  async function handleOpenExternal() {
    if (!canAct) return;
    closeMenu();
    await openUrlInDefaultBrowser(pageUrl);
  }

  function handleFindInPage() {
    closeMenu();
    humanBrowser.openFindBar();
  }

  async function handleReopenTab() {
    closeMenu();
    await humanBrowser.reopenClosedTab();
  }

  function handleHandBack() {
    closeMenu();
    browser.handBackToAgent();
  }

  function closeSaved() {
    savedOpen = false;
  }
</script>

{#if mobile}
  <button
    bind:this={menuAnchorEl}
    type="button"
    class="btn btn-icon btn-sm shrink-0"
    aria-label="Page actions"
    title="Page actions"
    data-browser-popover-trigger
    data-browser-more-trigger
    aria-expanded={menuOpen || savedOpen}
    onclick={toggleMenu}
  >
    <Ellipsis size={18} />
  </button>

  <BrowserPopover
    open={menuOpen}
    onClose={closeMenu}
    anchorRect={menuAnchorRect}
    placement="above"
    title="Page actions"
    ariaLabel="Browser page actions"
    width={280}
    maxHeight={320}
    hideNativeEmbed={true}
    backdrop={true}
  >
    <button
      type="button"
      class="browser-popover-row disabled:opacity-40"
      disabled={!canAct}
      onclick={() => void handleCopyUrl()}
    >
      <Copy size={16} class="shrink-0 text-surface-400" />
      <span class="text-sm text-surface-50">Copy link</span>
    </button>
    <button
      type="button"
      class="browser-popover-row disabled:opacity-40"
      disabled={!canAct}
      onclick={() => void handleOpenExternal()}
    >
      <ExternalLink size={16} class="shrink-0 text-surface-400" />
      <span class="text-sm text-surface-50">Open in browser</span>
    </button>
    <button type="button" class="browser-popover-row" onclick={handleFindInPage}>
      <Search size={16} class="shrink-0 text-surface-400" />
      <span class="text-sm text-surface-50">Find in page</span>
    </button>
    <button type="button" class="browser-popover-row" onclick={() => void handleReopenTab()}>
      <RotateCcw size={16} class="shrink-0 text-surface-400" />
      <span class="text-sm text-surface-50">Reopen closed tab</span>
    </button>
    {#if showHandBack}
      <button type="button" class="browser-popover-row" onclick={handleHandBack}>
        <Undo2 size={16} class="shrink-0 text-surface-400" />
        <span class="text-sm text-surface-50">Hand back to Medousa</span>
      </button>
    {/if}
    <button
      type="button"
      class="browser-popover-row disabled:opacity-40"
      disabled={!onReload}
      onclick={() => void handleReloadFromMenu()}
    >
      <RefreshCw size={16} class="shrink-0 text-surface-400" />
      <span class="text-sm text-surface-50">Reload</span>
    </button>
    <button
      type="button"
      class="browser-popover-row disabled:opacity-40"
      disabled={!canAct}
      onclick={(event) => void handleToggleStar(event)}
    >
      <Star
        size={16}
        class="shrink-0 {starred ? 'text-amber-400' : 'text-surface-400'}"
        fill={starred ? "currentColor" : "none"}
      />
      <span class="text-sm text-surface-50">
        {starred ? "Remove bookmark" : "Bookmark page"}
      </span>
    </button>
    <button
      type="button"
      class="browser-popover-row disabled:opacity-40"
      disabled={!canAct || saving}
      onclick={(event) => void handleSaveToLibrary(event)}
    >
      <BookmarkPlus size={16} class="shrink-0 text-surface-400" />
      <span class="text-sm text-surface-50">Save to Library</span>
    </button>
    <button type="button" class="browser-popover-row" onclick={openSavedFromMenu}>
      <ChevronDown size={16} class="shrink-0 text-surface-400" />
      <span class="text-sm text-surface-50">Saved pages</span>
    </button>
  </BrowserPopover>

  <BrowserSavedSheet
    open={savedOpen}
    onClose={closeSaved}
    anchorRect={menuAnchorRect}
    mobile
    placement="above"
  />
{:else}
  <div class="relative flex shrink-0 items-center gap-1">
    <button
      type="button"
      class="btn btn-icon btn-sm {starred ? 'text-amber-400' : ''}"
      aria-label={starred ? "Remove bookmark" : "Bookmark page"}
      title={starred ? "Remove bookmark" : "Bookmark page"}
      disabled={!canAct}
      onclick={(event) => void handleToggleStar(event)}
    >
      <Star size={16} fill={starred ? "currentColor" : "none"} />
    </button>

    <button
      type="button"
      class="btn btn-sm variant-soft-surface shrink-0"
      disabled={!canAct || saving}
      onclick={(event) => void handleSaveToLibrary(event)}
      title="Save page to Library"
    >
      <BookmarkPlus size={14} class="mr-1 inline" />
      Save
    </button>

    <button
      bind:this={menuAnchorEl}
      type="button"
      class="btn btn-icon btn-sm"
      aria-label="More actions"
      title="More actions"
      onclick={toggleMenu}
    >
      <Ellipsis size={16} />
    </button>

    <BrowserPopover
      open={menuOpen}
      onClose={closeMenu}
      anchorRect={menuAnchorRect}
      placement="below"
      title="Page actions"
      ariaLabel="Browser page actions"
      width={260}
      maxHeight={360}
      hideNativeEmbed={true}
      backdrop={true}
    >
      <button
        type="button"
        class="browser-popover-row disabled:opacity-40"
        disabled={!canAct}
        onclick={() => void handleCopyUrl()}
      >
        <Copy size={16} class="shrink-0 text-surface-400" />
        <span class="text-sm text-surface-50">Copy link</span>
      </button>
      <button
        type="button"
        class="browser-popover-row disabled:opacity-40"
        disabled={!canAct}
        onclick={() => void handleOpenExternal()}
      >
        <ExternalLink size={16} class="shrink-0 text-surface-400" />
        <span class="text-sm text-surface-50">Open in default browser</span>
      </button>
      <button type="button" class="browser-popover-row" onclick={handleFindInPage}>
        <Search size={16} class="shrink-0 text-surface-400" />
        <span class="text-sm text-surface-50">Find in page</span>
      </button>
      <button type="button" class="browser-popover-row" onclick={() => void handleReopenTab()}>
        <RotateCcw size={16} class="shrink-0 text-surface-400" />
        <span class="text-sm text-surface-50">Reopen closed tab</span>
      </button>
      {#if showHandBack}
        <button type="button" class="browser-popover-row" onclick={handleHandBack}>
          <Undo2 size={16} class="shrink-0 text-surface-400" />
          <span class="text-sm text-surface-50">Hand back to Medousa</span>
        </button>
      {/if}
    </BrowserPopover>

    <button
      bind:this={savedAnchorEl}
      type="button"
      class="btn btn-icon btn-sm"
      aria-label="Bookmarks"
      title="Bookmarks"
      data-browser-popover-trigger
      data-browser-saved-trigger
      aria-expanded={savedOpen}
      onclick={handleSavedToggle}
    >
      <ChevronDown size={16} />
    </button>

    <BrowserSavedSheet
      open={savedOpen}
      onClose={closeSaved}
      anchorRect={savedAnchorRect}
      placement="panel"
    />
  </div>
{/if}
