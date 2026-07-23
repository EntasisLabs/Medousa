<script lang="ts">
  import { FolderPlus, RefreshCw, Search, X } from "@lucide/svelte";
  import { onMount, tick } from "svelte";
  import ExternalFilesBrowser from "$lib/components/vault/ExternalFilesBrowser.svelte";
  import ExternalFileRow from "$lib/components/vault/ExternalFileRow.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { portLmeDock } from "$lib/utils/lmeDockHost";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { canPreviewAttachment } from "$lib/utils/vaultAttachments";
  import {
    isCoLocatedWorkshop,
    vaultPinFolderRemoteHint,
  } from "$lib/utils/workshopLocality";
  import type { ExternalFileEntry } from "$lib/types/externalDesk";

  let searchExpanded = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  const externalHits = $derived(externalDesk.searchHitsList);
  const query = $derived(vault.searchQuery);
  const searching = $derived(query.trim().length > 0);
  const canLinkFiles = $derived(Boolean(vault.selectedPath));
  const coLocated = $derived(isCoLocatedWorkshop());
  const hasFolders = $derived(externalDesk.pinnedRoots.length > 0);
  const refreshing = $derived(Boolean(externalDesk.loadingRoot));

  onMount(() => {
    if (hasFolders) {
      void externalDesk.refreshAllRoots();
    }
  });

  $effect(() => {
    if (searching && !searchExpanded) {
      searchExpanded = true;
    }
  });

  async function openSearch() {
    searchExpanded = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchExpanded = false;
    handleExternalSearch("");
  }

  function handleExternalSearch(value: string) {
    vault.searchQuery = value;
    externalDesk.setSearchQuery(value);
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSearch();
    }
  }

  async function handleOpen(entry: ExternalFileEntry) {
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

<aside class="lme-files-explorer flex h-full min-h-0 w-full flex-col" aria-label="Local Files">
  <div class="min-h-0 flex-1 overflow-hidden">
    {#if searching}
      <div class="flex h-full min-h-0 flex-col overflow-y-auto px-1.5 py-1">
        {#if externalHits.length === 0}
          <p class="px-2 py-4 text-sm text-surface-500">No files match.</p>
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

  <footer
    class="lme-side-rail-dock lme-files-dock"
    use:portLmeDock
  >
    {#if searchExpanded}
      <div class="lme-dock-search-expand min-w-0 flex-1">
        <Search size={14} strokeWidth={1.75} class="lme-dock-search-glyph" />
        <input
          bind:this={searchInputEl}
          class="lme-dock-search-input"
          type="search"
          placeholder="Search files…"
          value={query}
          oninput={(event) =>
            handleExternalSearch((event.currentTarget as HTMLInputElement).value)}
          onkeydown={handleSearchKeydown}
        />
      </div>
    {:else}
      <div class="min-w-0 flex-1">
        {#if !coLocated}
          <span class="workshop-faint truncate text-[11px]" title={vaultPinFolderRemoteHint()}>
            Local only
          </span>
        {/if}
      </div>
    {/if}

    {#if coLocated}
      <button
        type="button"
        class="vault-dock-icon-btn"
        aria-label="Add folder"
        title="Add folder"
        onclick={() => void externalDesk.pinFolder()}
      >
        <FolderPlus size={15} strokeWidth={1.75} />
      </button>
    {/if}

    {#if hasFolders}
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
    {/if}

    {#if searchExpanded}
      <button
        type="button"
        class="vault-dock-icon-btn"
        aria-label="Close search"
        title="Close search"
        onclick={closeSearch}
      >
        <X size={15} strokeWidth={1.75} />
      </button>
    {:else}
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
  </footer>
</aside>
