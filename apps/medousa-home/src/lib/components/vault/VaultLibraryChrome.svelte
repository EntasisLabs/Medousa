<script lang="ts">
  import { PanelLeftClose, Search, Trash2, X } from "@lucide/svelte";
  import { tick } from "svelte";
  import { layout } from "$lib/runtime/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { titleWithShortcut } from "$lib/utils/keyboardShortcutsCatalog";
  import VaultCreateMenuItems from "./VaultCreateMenuItems.svelte";
  import VaultGroupPicker from "./VaultGroupPicker.svelte";
  import VaultRootPicker from "./VaultRootPicker.svelte";
  import VaultLibraryBrowseModeBar from "./VaultLibraryBrowseModeBar.svelte";
  import VaultTrashPanel from "./VaultTrashPanel.svelte";
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
  let trashOpen = $state(false);
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
          aria-selected={externalDesk.sidebarMode === "artifacts"}
          class="vault-sidebar-tab {externalDesk.sidebarMode === 'artifacts'
            ? 'vault-sidebar-tab-active'
            : ''}"
          onclick={() => externalDesk.setSidebarMode("artifacts")}
        >
          Artifacts
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
            title={titleWithShortcut("New note", "vault-new")}
            onclick={(event) => {
              event.stopPropagation();
              createOpen = !createOpen;
            }}
          >
            + New
          </button>
          {#if createOpen}
            <div
              class="vault-create-menu absolute right-0 top-full z-30 mt-1"
              role="menu"
              tabindex="-1"
              onclick={(event) => event.stopPropagation()}
              onkeydown={handleMenuKeydown}
            >
              <VaultCreateMenuItems onClose={closeMenus} />
            </div>
          {/if}
        </div>

        <VaultLibraryBrowseModeBar icons flush />

        <button
          type="button"
          class="vault-dock-icon-btn {trashOpen ? 'vault-dock-icon-btn-active' : ''}"
          aria-label="Trash"
          title="Trash"
          onclick={() => {
            closeMenus();
            trashOpen = !trashOpen;
          }}
        >
          <Trash2 size={15} strokeWidth={1.75} />
        </button>

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
            class="workshop-text-action text-xs text-content-quiet"
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

<VaultTrashPanel open={trashOpen} onClose={() => (trashOpen = false)} />
