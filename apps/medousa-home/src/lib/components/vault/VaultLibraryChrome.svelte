<script lang="ts">
  import {
    Calendar,
    CalendarRange,
    FilePlus,
    FolderPlus,
    PanelLeftClose,
    Plus,
    Search,
    SlidersHorizontal,
  } from "@lucide/svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { vaultQuickSwitcher } from "$lib/stores/vaultQuickSwitcher.svelte";
  import VaultRootPicker from "./VaultRootPicker.svelte";
  import VaultSpaceChips from "./VaultSpaceChips.svelte";

  interface Props {
    showVaultChrome: boolean;
    onSearchExternal?: (query: string) => void;
  }

  let { showVaultChrome, onSearchExternal }: Props = $props();

  let createOpen = $state(false);
  let filtersOpen = $state(false);

  const filterCount = $derived(
    Number(vault.showAgentReviewFilter) + Number(vault.showSystemNotes),
  );

  function closeMenus() {
    createOpen = false;
    filtersOpen = false;
  }
</script>

<svelte:window onclick={closeMenus} />

<div class="vault-browser-chrome shrink-0 border-b border-surface-500/45 bg-surface-800/50">
  <div class="flex items-center gap-1 border-b border-surface-500/35 px-2 pt-2">
    <div class="flex min-w-0 flex-1 gap-0 pl-1">
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
    </div>
    <button
      type="button"
      class="btn btn-xs variant-ghost-surface mt-0.5 shrink-0"
      title="Hide library browser"
      aria-label="Hide library browser"
      onclick={() => layout.setVaultSidebarCollapsed(true)}
    >
      <PanelLeftClose size={14} strokeWidth={2} />
    </button>
  </div>

  <div class="flex items-center gap-2 px-3 py-2.5">
    <h2 class="text-sm font-semibold text-surface-50">Library</h2>
    {#if showVaultChrome}
      <VaultRootPicker compact />
    {/if}
  </div>

  <div class="px-3 pb-2">
    {#if showVaultChrome}
      <button
        type="button"
        class="vault-search-trigger w-full"
        onclick={() => vaultQuickSwitcher.openSwitcher()}
      >
        <Search size={14} strokeWidth={2} class="shrink-0 opacity-60" />
        <span class="truncate text-surface-400">Find note…</span>
        <kbd class="vault-search-kbd ml-auto">⌘O</kbd>
      </button>
    {:else}
      <label class="vault-search-trigger cursor-text">
        <Search size={14} strokeWidth={2} class="shrink-0 opacity-60" />
        <input
          class="min-w-0 flex-1 border-0 bg-transparent p-0 text-sm text-surface-100 outline-none placeholder:text-surface-500"
          type="search"
          placeholder="Search pinned folders…"
          value={vault.searchQuery}
          oninput={(event) => onSearchExternal?.((event.currentTarget as HTMLInputElement).value)}
        />
      </label>
    {/if}
  </div>

  {#if showVaultChrome}
    <div class="flex items-center gap-1.5 px-3 pb-2.5">
      <div class="relative">
        <button
          type="button"
          class="btn btn-sm variant-filled-primary"
          aria-haspopup="menu"
          aria-expanded={createOpen}
          onclick={(event) => {
            event.stopPropagation();
            filtersOpen = false;
            createOpen = !createOpen;
          }}
        >
          <Plus size={14} strokeWidth={2} />
          New
        </button>
        {#if createOpen}
          <div
            class="absolute left-0 top-full z-30 mt-1 min-w-[11rem] rounded-lg border border-surface-500/50 bg-surface-900 py-1 shadow-xl"
            role="menu"
            onclick={(event) => event.stopPropagation()}
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
              class="vault-menu-item"
              onclick={() => {
                closeMenus();
                vault.openNewNoteDialog();
              }}
            >
              <FilePlus size={14} strokeWidth={2} />
              New note
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
          </div>
        {/if}
      </div>

      <div class="relative ml-auto">
        <button
          type="button"
          class="btn btn-sm variant-ghost-surface {filterCount > 0 ? 'text-primary-200' : ''}"
          aria-haspopup="menu"
          aria-expanded={filtersOpen}
          title="View filters"
          onclick={(event) => {
            event.stopPropagation();
            createOpen = false;
            filtersOpen = !filtersOpen;
          }}
        >
          <SlidersHorizontal size={14} strokeWidth={2} />
          {#if filterCount > 0}
            <span class="tabular-nums">{filterCount}</span>
          {/if}
        </button>
        {#if filtersOpen}
          <div
            class="absolute right-0 top-full z-30 mt-1 min-w-[12rem] rounded-lg border border-surface-500/50 bg-surface-900 p-2 shadow-xl"
            role="menu"
            onclick={(event) => event.stopPropagation()}
          >
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
    </div>

    <div class="border-t border-surface-500/35 px-3 py-2">
      <VaultSpaceChips embedded compact />
    </div>
  {/if}
</div>
