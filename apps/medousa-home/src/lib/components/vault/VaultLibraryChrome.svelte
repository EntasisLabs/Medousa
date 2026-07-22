<script lang="ts">
  import {
    Calendar,
    CalendarRange,
    FilePlus,
    FileText,
    FolderPlus,
    PanelLeftClose,
    Search,
    X,
  } from "@lucide/svelte";
  import { tick } from "svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { formatShortcut } from "$lib/platform";
  import { canUseLocalVaultFilesystem } from "$lib/utils/vaultFilesystem";
  import VaultGroupPicker from "./VaultGroupPicker.svelte";
  import VaultRootPicker from "./VaultRootPicker.svelte";
  import VaultLibraryBrowseModeBar from "./VaultLibraryBrowseModeBar.svelte";
  import { isCoLocatedWorkshop, vaultPinFolderRemoteHint } from "$lib/utils/workshopLocality";

  interface Props {
    showVaultChrome: boolean;
    onSearchExternal?: (query: string) => void;
    /** When true, Vault/Files/Decks tabs are owned by the LME mode bar. */
    hideLibraryTabs?: boolean;
  }

  let { showVaultChrome, onSearchExternal, hideLibraryTabs = false }: Props = $props();

  let createOpen = $state(false);
  let searchExpanded = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);

  const coLocated = $derived(isCoLocatedWorkshop());
  const searching = $derived(vault.searchQuery.trim().length > 0);

  $effect(() => {
    if (searching && !searchExpanded && showVaultChrome) {
      searchExpanded = true;
    }
  });

  function closeMenus() {
    createOpen = false;
  }

  function handleMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenus();
    }
  }

  async function openSearch() {
    closeMenus();
    searchExpanded = true;
    await tick();
    searchInputEl?.focus();
  }

  function closeSearch() {
    searchExpanded = false;
    if (searching) void vault.runSearch("");
  }

  function handleSearchKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeSearch();
    }
  }
</script>

<svelte:window onclick={closeMenus} />

<div
  class="vault-browser-chrome shrink-0 {hideLibraryTabs
    ? 'border-b border-surface-500/25'
    : 'border-b border-surface-500/45 bg-surface-800/50'}"
>
  {#if !hideLibraryTabs}
    <div class="vault-library-tabbar">
      <div class="vault-library-tabbar-tabs pl-1">
        <button
          type="button"
          role="tab"
          aria-selected={externalDesk.sidebarMode === "vault"}
          class="vault-sidebar-tab {externalDesk.sidebarMode === 'vault'
            ? 'vault-sidebar-tab-active'
            : ''}"
          onclick={() => externalDesk.setSidebarMode("vault")}
        >
          Vault
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={externalDesk.sidebarMode === "files"}
          class="vault-sidebar-tab {externalDesk.sidebarMode === 'files'
            ? 'vault-sidebar-tab-active'
            : ''}"
          onclick={() => externalDesk.setSidebarMode("files")}
        >
          Your files
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={externalDesk.sidebarMode === "presentations"}
          class="vault-sidebar-tab {externalDesk.sidebarMode === 'presentations'
            ? 'vault-sidebar-tab-active'
            : ''}"
          onclick={() => externalDesk.setSidebarMode("presentations")}
        >
          Presentations
        </button>
      </div>
      <button
        type="button"
        class="vault-toolbar-btn my-1.5"
        title="Hide library browser"
        aria-label="Hide library browser"
        onclick={() => layout.setVaultSidebarCollapsed(true)}
      >
        <PanelLeftClose size={14} strokeWidth={2} />
      </button>
    </div>
  {/if}

  {#if showVaultChrome}
    <div class="flex items-center gap-0.5 px-1.5 py-1">
      {#if searchExpanded}
        <div class="lme-dock-search-expand min-w-0 flex-1">
          <Search size={14} strokeWidth={1.75} class="lme-dock-search-glyph" />
          <input
            bind:this={searchInputEl}
            class="lme-dock-search-input"
            type="search"
            placeholder="Search notes…"
            value={vault.searchQuery}
            oninput={(event) =>
              void vault.runSearch((event.currentTarget as HTMLInputElement).value)}
            onkeydown={handleSearchKeydown}
          />
        </div>
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
        <div class="flex min-w-0 items-center gap-0.5">
          <VaultRootPicker compact quiet />
          <VaultGroupPicker />
        </div>
        <div class="min-w-1 flex-1"></div>

        <div class="relative shrink-0">
          <button
            type="button"
            class="workshop-text-action px-1.5 text-xs"
            aria-haspopup="menu"
            aria-expanded={createOpen}
            onclick={(event) => {
              event.stopPropagation();
              createOpen = !createOpen;
            }}
          >
            + New
          </button>
          {#if createOpen}
            <div
              class="absolute right-0 top-full z-30 mt-1 min-w-[11rem] rounded-lg border border-surface-500/50 bg-surface-900 py-1 shadow-xl"
              role="menu"
              tabindex="-1"
              onclick={(event) => event.stopPropagation()}
              onkeydown={handleMenuKeydown}
            >
              <button
                type="button"
                role="menuitem"
                class="vault-menu-item"
                disabled={vault.saving}
                onclick={() => {
                  closeMenus();
                  void vault.createDailyNote();
                }}
              >
                <Calendar size={14} strokeWidth={2} />
                Daily note
              </button>
              <button
                type="button"
                role="menuitem"
                class="vault-menu-item"
                disabled={vault.saving}
                onclick={() => {
                  closeMenus();
                  void vault.createWeeklyReview();
                }}
              >
                <CalendarRange size={14} strokeWidth={2} />
                Weekly review
              </button>
              <button
                type="button"
                role="menuitem"
                class="vault-menu-item w-full justify-between"
                onclick={() => {
                  closeMenus();
                  vault.openNewNoteDialog();
                }}
              >
                <span class="inline-flex items-center gap-2">
                  <FilePlus size={14} strokeWidth={2} />
                  New note
                </span>
                <kbd class="vault-kbd">{formatShortcut("N")}</kbd>
              </button>
              <button
                type="button"
                role="menuitem"
                class="vault-menu-item"
                onclick={() => {
                  closeMenus();
                  vault.openNewGroupDialog();
                }}
              >
                <FolderPlus size={14} strokeWidth={2} />
                New group
              </button>
              {#if canUseLocalVaultFilesystem()}
                <div class="my-1 border-t border-surface-500/35"></div>
                <button
                  type="button"
                  role="menuitem"
                  class="vault-menu-item"
                  onclick={() => {
                    closeMenus();
                    void vault.openLooseMarkdownFile();
                  }}
                >
                  <FileText size={14} strokeWidth={2} />
                  Open markdown file…
                </button>
              {/if}
            </div>
          {/if}
        </div>

        <VaultLibraryBrowseModeBar icons flush />

        <button
          type="button"
          class="vault-dock-icon-btn {searching ? 'vault-dock-icon-btn-active' : ''}"
          aria-label="Search notes"
          title="Search"
          onclick={() => void openSearch()}
        >
          <Search size={15} strokeWidth={1.75} />
        </button>
      {/if}
    </div>
  {:else}
    <div class="px-3 py-2">
      <input
        class="input w-full text-xs"
        type="search"
        placeholder="Search pinned folders…"
        value={vault.searchQuery}
        oninput={(event) => onSearchExternal?.((event.currentTarget as HTMLInputElement).value)}
      />
    </div>
    <div class="flex flex-wrap items-center gap-3 px-3 pb-2">
      {#if coLocated}
        <button
          type="button"
          class="workshop-text-action text-xs"
          onclick={() => void externalDesk.pinFolder()}
        >
          + Pin folder
        </button>
        {#if externalDesk.pinnedRoots.length > 0}
          <button
            type="button"
            class="workshop-text-action text-xs text-surface-500"
            disabled={Boolean(externalDesk.loadingRoot)}
            onclick={() => void externalDesk.refreshAllRoots()}
          >
            Refresh
          </button>
        {/if}
      {:else}
        <p class="workshop-faint text-[11px] leading-snug">
          {vaultPinFolderRemoteHint()}
        </p>
      {/if}
    </div>
  {/if}
</div>
