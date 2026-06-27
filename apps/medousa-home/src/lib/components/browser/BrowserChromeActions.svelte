<script lang="ts">
  import { BookmarkPlus, ChevronDown, Star } from "@lucide/svelte";
  import { browserBookmarks } from "$lib/stores/browserBookmarks.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import BrowserSavedSheet from "$lib/components/browser/BrowserSavedSheet.svelte";
  import { normalizeBrowserUrl } from "$lib/utils/browserUrl";
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
  }

  let { mobile = false, onMobileToast }: Props = $props();

  let saving = $state(false);
  let savedOpen = $state(false);
  let menuAnchorEl = $state<HTMLButtonElement | null>(null);
  let menuAnchorRect = $state<DOMRect | null>(null);

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

  async function handleToggleStar(event: MouseEvent) {
    event.stopPropagation();
    if (!canAct) return;
    const result = await togglePageBookmark(pageUrl, pageTitle);
    if (mobile && onMobileToast) {
      onMobileToast(result === "added" ? "Bookmarked" : "Bookmark removed");
    } else {
      showBookmarkFeedback(result);
    }
  }

  async function handleSaveToLibrary(event: MouseEvent) {
    event.stopPropagation();
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
      menuAnchorRect = menuAnchorEl?.getBoundingClientRect() ?? null;
    }
    savedOpen = !savedOpen;
  }

  function closeSaved() {
    savedOpen = false;
  }
</script>

<div class="relative flex shrink-0 items-center gap-1">
  <button
    type="button"
    class="btn btn-icon btn-sm {starred ? 'text-amber-400' : ''}"
    aria-label={starred ? "Remove bookmark" : "Bookmark page"}
    title={starred ? "Remove bookmark" : "Bookmark page"}
    disabled={!canAct}
    onclick={(event) => void handleToggleStar(event)}
  >
    <Star size={mobile ? 18 : 16} fill={starred ? "currentColor" : "none"} />
  </button>

  {#if mobile}
    <button
      type="button"
      class="btn btn-icon btn-sm"
      aria-label="Save to Library"
      title="Save to Library"
      disabled={!canAct || saving}
      onclick={(event) => void handleSaveToLibrary(event)}
    >
      <BookmarkPlus size={18} />
    </button>
  {:else}
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
  {/if}

  <button
    bind:this={menuAnchorEl}
    type="button"
    class="btn btn-icon btn-sm"
    aria-label="Bookmarks"
    title="Bookmarks"
    data-browser-popover-trigger
    data-browser-saved-trigger
    aria-expanded={savedOpen}
    onclick={handleSavedToggle}
  >
    <ChevronDown size={mobile ? 18 : 16} />
  </button>

  <BrowserSavedSheet
    open={savedOpen}
    onClose={closeSaved}
    anchorRect={menuAnchorRect}
    {mobile}
    placement={mobile ? "above" : "panel"}
  />
</div>
