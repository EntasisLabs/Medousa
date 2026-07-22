<script lang="ts">
  import {
    Calendar,
    CalendarRange,
    Check,
    FilePlus,
    FileText,
    FolderPlus,
    PanelLeftClose,
    SlidersHorizontal,
  } from "@lucide/svelte";
  import { allFilterSpaces } from "$lib/config/vaultSpaces";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { iconForSpace } from "$lib/utils/vaultSpaceIcons";
  import { formatShortcut } from "$lib/platform";
  import { canUseLocalVaultFilesystem } from "$lib/utils/vaultFilesystem";
  import VaultRootPicker from "./VaultRootPicker.svelte";
  import VaultLibraryBrowseModeBar from "./VaultLibraryBrowseModeBar.svelte";
  import { vaultFolderIcons } from "$lib/stores/vaultFolderIcons.svelte";
  import { isCoLocatedWorkshop, vaultPinFolderRemoteHint } from "$lib/utils/workshopLocality";

  interface Props {
    showVaultChrome: boolean;
    onSearchExternal?: (query: string) => void;
    /** When true, Vault/Files/Decks tabs are owned by the LME mode bar. */
    hideLibraryTabs?: boolean;
  }

  let { showVaultChrome, onSearchExternal, hideLibraryTabs = false }: Props = $props();

  let createOpen = $state(false);
  let filtersOpen = $state(false);

  // Keep filter icons reactive to custom folder icon picks.
  const folderIconMap = $derived(vaultFolderIcons.icons);
  const coLocated = $derived(isCoLocatedWorkshop());

  const visibleSpaces = $derived(allFilterSpaces(vault.showSystemNotes));
  const spaceCounts = $derived(vault.spaceCountsMap);

  const viewFilterCount = $derived(
    Number(vault.showAgentReviewFilter) + Number(vault.showSystemNotes),
  );

  const menuActiveCount = $derived(
    viewFilterCount + Number(vault.activeSpaceFilter !== null),
  );

  function closeMenus() {
    createOpen = false;
    filtersOpen = false;
  }

  function handleMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenus();
    }
  }

  function selectSpace(spaceId: string | null) {
    vault.setActiveSpaceFilter(spaceId);
    filtersOpen = false;
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
      <div class="flex items-center gap-2 px-3 py-2">
        <div class="min-w-0 flex-1">
          <VaultRootPicker compact />
        </div>

        <div class="flex shrink-0 items-center gap-2">
          <div class="relative">
            <button
              type="button"
              class="workshop-text-action text-xs {menuActiveCount > 0
                ? 'text-primary-300'
                : 'text-surface-500'}"
              aria-haspopup="menu"
              aria-expanded={filtersOpen}
              title="Filter by group"
              onclick={(event) => {
                event.stopPropagation();
                createOpen = false;
                filtersOpen = !filtersOpen;
              }}
            >
              <span class="inline-flex items-center gap-1">
                <SlidersHorizontal size={12} strokeWidth={2} />
                Filter
              </span>
            </button>
            {#if filtersOpen}
              <div
                class="vault-library-filter-menu absolute right-0 top-full z-30 mt-1 rounded-lg border border-surface-500/50 bg-surface-900 py-1 shadow-xl"
                role="menu"
                tabindex="-1"
                onclick={(event) => event.stopPropagation()}
                onkeydown={handleMenuKeydown}
              >
                <p class="px-3 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
                  Group
                </p>
                <button
                  type="button"
                  role="menuitemradio"
                  aria-checked={vault.activeSpaceFilter === null}
                  class="vault-menu-item w-full justify-between {vault.activeSpaceFilter === null
                    ? 'text-primary-200'
                    : ''}"
                  onclick={() => selectSpace(null)}
                >
                  <span>All notes</span>
                  {#if vault.activeSpaceFilter === null}
                    <Check size={14} strokeWidth={2} class="text-primary-300" />
                  {/if}
                </button>
                {#each visibleSpaces as space (space.id)}
                  {@const _ = folderIconMap}
                  {@const Icon = iconForSpace(space.id)}
                  {@const count = spaceCounts.get(space.id) ?? 0}
                  <button
                    type="button"
                    role="menuitemradio"
                    aria-checked={vault.activeSpaceFilter === space.id}
                    class="vault-menu-item w-full justify-between {vault.activeSpaceFilter ===
                    space.id
                      ? 'text-primary-200'
                      : ''}"
                    onclick={() => selectSpace(space.id)}
                  >
                    <span class="inline-flex min-w-0 items-center gap-2">
                      <Icon size={14} strokeWidth={2} class="shrink-0 opacity-80" />
                      <span class="truncate">{space.label}</span>
                      {#if count > 0}
                        <span class="workshop-faint tabular-nums">{count}</span>
                      {/if}
                    </span>
                    {#if vault.activeSpaceFilter === space.id}
                      <Check size={14} strokeWidth={2} class="shrink-0 text-primary-300" />
                    {/if}
                  </button>
                {/each}

                <div class="my-1 border-t border-surface-500/35"></div>

                <p class="px-3 pb-1 pt-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
                  View
                </p>
                <button
                  type="button"
                  role="menuitemcheckbox"
                  aria-checked={vault.showAgentReviewFilter}
                  class="vault-menu-item w-full justify-between {vault.showAgentReviewFilter
                    ? 'text-primary-200'
                    : ''}"
                  onclick={() => vault.setShowAgentReviewFilter(!vault.showAgentReviewFilter)}
                >
                  Agent review
                  {#if vault.showAgentReviewFilter}
                    <span class="text-[10px] text-primary-300">On</span>
                  {/if}
                </button>
                <button
                  type="button"
                  role="menuitemcheckbox"
                  aria-checked={vault.showSystemNotes}
                  class="vault-menu-item w-full justify-between {vault.showSystemNotes
                    ? 'text-primary-200'
                    : ''}"
                  onclick={() => vault.setShowSystemNotes(!vault.showSystemNotes)}
                >
                  Developer notes
                  {#if vault.showSystemNotes}
                    <span class="text-[10px] text-primary-300">On</span>
                  {/if}
                </button>
              </div>
            {/if}
          </div>

          <div class="relative">
            <button
              type="button"
              class="workshop-text-action text-xs"
              aria-haspopup="menu"
              aria-expanded={createOpen}
              onclick={(event) => {
                event.stopPropagation();
                filtersOpen = false;
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
        </div>
      </div>
      <VaultLibraryBrowseModeBar />
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
