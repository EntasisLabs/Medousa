<script lang="ts">
  import { ExternalLink, FileText, History, Star } from "@lucide/svelte";
  import BrowserPopover from "$lib/components/browser/BrowserPopover.svelte";
  import { browserHistory } from "$lib/stores/browserHistory.svelte";
  import { humanBrowser } from "$lib/stores/humanBrowser.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import type { BrowserBookmark } from "$lib/stores/browserBookmarks.svelte";
  import {
    loadSavedPages,
    openSavedVaultNote,
    type VaultBrowserSave,
  } from "$lib/utils/saveBrowserPage";
  import type { PopoverPlacement } from "$lib/utils/browserPopoverOverlay";

  interface Props {
    open?: boolean;
    onClose?: () => void;
    anchorRect?: DOMRect | null;
    placement?: PopoverPlacement;
    mobile?: boolean;
  }

  let {
    open = false,
    onClose,
    anchorRect = null,
    placement = "panel",
    mobile = false,
  }: Props = $props();

  let loading = $state(false);
  let quick = $state<BrowserBookmark[]>([]);
  let library = $state<VaultBrowserSave[]>([]);

  const resolvedPlacement = $derived<PopoverPlacement>(
    mobile ? "above" : placement,
  );

  const historyEntries = $derived(browserHistory.recent(8));

  $effect(() => {
    if (!open) return;
    loading = true;
    void loadSavedPages()
      .then((result) => {
        quick = result.quick;
        library = result.library;
      })
      .finally(() => {
        loading = false;
      });
  });

  function navigate(url: string) {
    void humanBrowser.navigate(url);
    onClose?.();
  }

  function openNote(path: string) {
    if (layout.isMobile) {
      layout.openNotes({ view: "reader" });
    } else {
      layout.navigateDesktop("library");
    }
    openSavedVaultNote(path);
    onClose?.();
  }
</script>

<BrowserPopover
  {open}
  {onClose}
  {anchorRect}
  placement={resolvedPlacement}
  title="Bookmarks"
  ariaLabel="Saved pages"
  width={mobile ? 340 : 360}
  maxHeight={mobile ? 320 : 420}
  hideNativeEmbed={true}
  backdrop={true}
>
  {#if loading}
    <p class="px-3 py-4 text-sm text-surface-400">Loading…</p>
  {:else if quick.length === 0 && library.length === 0 && historyEntries.length === 0}
    <p class="px-4 py-6 text-center text-sm text-surface-400">
      Star pages or save to Library to see them here.
    </p>
  {:else}
    {#if historyEntries.length > 0}
      <p class="browser-popover-section-label">History</p>
      {#each historyEntries as entry (entry.url + entry.visitedAt)}
        <button
          type="button"
          class="browser-popover-row"
          onclick={() => navigate(entry.url)}
        >
          <History size={15} class="shrink-0 text-surface-400" />
          <span class="min-w-0 flex-1">
            <span class="block truncate text-sm text-surface-50">{entry.title || entry.url}</span>
            <span class="block truncate text-[11px] text-surface-400">{entry.url}</span>
          </span>
          <ExternalLink size={12} class="shrink-0 text-surface-500" />
        </button>
      {/each}
    {/if}

    {#if quick.length > 0}
      <p class="browser-popover-section-label">Quick bookmarks</p>
      {#each quick as entry (entry.url)}
        <button
          type="button"
          class="browser-popover-row"
          onclick={() => navigate(entry.url)}
        >
          <Star size={15} class="shrink-0 text-amber-400" />
          <span class="min-w-0 flex-1">
            <span class="block truncate text-sm text-surface-50">{entry.title}</span>
            <span class="block truncate text-[11px] text-surface-400">{entry.url}</span>
          </span>
          <ExternalLink size={12} class="shrink-0 text-surface-500" />
        </button>
      {/each}
    {/if}

    {#if library.length > 0}
      <p class="browser-popover-section-label">Library saves</p>
      {#each library as entry (entry.path)}
        <div class="browser-popover-row-group">
          <button type="button" class="browser-popover-row flex-1" onclick={() => navigate(entry.url)}>
            <FileText size={15} class="shrink-0 text-surface-400" />
            <span class="min-w-0 flex-1">
              <span class="block truncate text-sm text-surface-50">{entry.title}</span>
              <span class="block truncate text-[11px] text-surface-400">{entry.url}</span>
            </span>
          </button>
          <button
            type="button"
            class="btn btn-icon btn-sm shrink-0"
            aria-label="Open note"
            title="Open note"
            onclick={() => openNote(entry.path)}
          >
            <FileText size={14} />
          </button>
        </div>
      {/each}
    {/if}
  {/if}
</BrowserPopover>
