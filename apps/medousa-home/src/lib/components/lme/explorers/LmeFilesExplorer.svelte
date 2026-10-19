<script lang="ts">
  import "$lib/styles/files-surface.postcss";
  import { FolderPlus, RefreshCw, Search, X } from "@lucide/svelte";
  import { tick } from "svelte";
  import ExternalFilesBrowser from "$lib/components/vault/ExternalFilesBrowser.svelte";
  import ExternalFileRow from "$lib/components/vault/ExternalFileRow.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { portLmeDock } from "$lib/utils/lmeDockHost";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { canPreviewAttachment } from "$lib/utils/vaultAttachments";
  import {
    isCoLocatedWorkshop,
    vaultPinFolderRemoteHint,
  } from "$lib/utils/workshopLocality";
  import type { ExternalFileEntry } from "$lib/types/externalDesk";

  let searchExpanded = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  const coLocated = $derived(isCoLocatedWorkshop());
  const externalHits = $derived(externalDesk.searchHitsList);
  const query = $derived(externalDesk.searchQuery);
  const searching = $derived(coLocated && query.trim().length > 0);
  const canLinkFiles = $derived(Boolean(vault.selectedPath));
  const hasFolders = $derived(externalDesk.pinnedRoots.length > 0);
  const refreshing = $derived(Boolean(externalDesk.loadingRoot));

  $effect(() => {
    if (searching && !searchExpanded) {
      searchExpanded = true;
    }
  });

  async function openSearch() {
    await ensureRailPopoverOpen();
    searchExpanded = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchExpanded = false;
    handleExternalSearch("");
  }

  function handleExternalSearch(value: string) {
    externalDesk.setSearchQuery(value);
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSearch();
    }
  }

  async function handleOpen(entry: ExternalFileEntry) {
    externalDesk.selectExternalPath(entry.path);
    const attachment = externalDesk.attachmentForPath(entry.path);
    if (canPreviewAttachment(attachment)) {
      lmeWorkspace.openFile(entry.path);
      return;
    }
    externalDesk.selectExternalPath(entry.path);
    await openAttachmentPath(entry.path);
  }

  function handleLink(entry: ExternalFileEntry) {
    if (!vault.selectedPath) return;
    vault.linkExternalFile(entry.path);
  }
</script>

<aside class="lme-files-explorer flex h-full min-h-0 w-full flex-col" aria-label="Files">
  <header
    class="lme-side-rail-dock lme-files-dock"
    use:portLmeDock
  >
    {#if searchExpanded}
      <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
        <Search size={14} strokeWidth={1.75} class="shrink-0 text-content-quiet" aria-hidden="true" />
        <input
          bind:this={searchInputEl}
          class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-content-quiet focus:outline-none focus:ring-0"
          type="search"
          placeholder="Filter loaded files…"
          value={query}
          oninput={(event) =>
            handleExternalSearch((event.currentTarget as HTMLInputElement).value)}
          onkeydown={handleSearchKeydown}
        />
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Close search"
          title="Close search"
          onclick={closeSearch}
        >
          <X size={14} strokeWidth={1.75} />
        </button>
      </div>
    {:else}
      <!-- Push action cluster to the right: New · Refresh · Search -->
      <span class="files-dock-title">Files</span>

      {#if !coLocated}
        <span
          class="workshop-faint mr-0.5 max-w-[4.5rem] shrink-0 truncate text-[10px]"
          title={vaultPinFolderRemoteHint()}
        >
          Local only
        </span>
      {/if}

      {#if coLocated}
        <button
          type="button"
          class="vault-dock-icon-btn"
          aria-label="Add folder"
          title="Add folder"
          onclick={() => void externalDesk.pinFolder()}
        >
          <FolderPlus size={16} strokeWidth={1.75} />
        </button>
      {/if}

      {#if hasFolders}
        <div class="lme-dock-chrome-secondary shrink-0">
          <button
            type="button"
            class="vault-dock-icon-btn"
            aria-label="Refresh folders"
            title="Refresh"
            disabled={refreshing}
            onclick={() => void externalDesk.refreshAllRoots()}
          >
            <RefreshCw
              size={15}
              strokeWidth={1.75}
              class={refreshing ? "animate-spin" : ""}
            />
          </button>
        </div>
      {/if}

      <button
        type="button"
        class="vault-dock-icon-btn"
        aria-label="Search files"
        title="Search"
        onclick={() => void openSearch()}
      >
        <Search size={15} strokeWidth={1.75} />
      </button>
    {/if}
  </header>
  <div class="min-h-0 flex-1 overflow-hidden">
    {#if searching}
      <div class="files-search-results flex h-full min-h-0 flex-col overflow-y-auto px-2 py-1">
        <div class="files-search-summary"><strong>{externalHits.length} {externalHits.length === 1 ? 'file' : 'files'}</strong><span>In loaded folders</span></div>
        {#if externalHits.length === 0}
          <div class="files-empty"><span class="files-empty-icon"><Search size={22} strokeWidth={1.5} /></span><h3>No files found</h3><p>Try another name, or open more folders to include their files.</p><button type="button" class="files-button files-button--quiet" onclick={closeSearch}>Back to folders</button></div>
        {:else}
          <ul class="space-y-0.5">
            {#each externalHits as entry (entry.path)}
              <li>
                <ExternalFileRow
                  {entry}
                  selected={externalDesk.selectedExternalPath === entry.path}
                  showLink={canLinkFiles}
                  onOpen={handleOpen}
                  onLink={handleLink}
                />
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    {:else}
      <ExternalFilesBrowser onOpenFile={handleOpen} />
    {/if}
  </div>

</aside>
