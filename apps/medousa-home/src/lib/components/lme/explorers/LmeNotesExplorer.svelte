<script lang="ts">
  import {
    Calendar,
    CalendarRange,
    Check,
    FilePlus,
    FileText,
    FolderPlus,
    Plus,
    SlidersHorizontal,
  } from "@lucide/svelte";
  import { onMount, tick, untrack } from "svelte";
  import VaultKindBadge from "$lib/components/vault/VaultKindBadge.svelte";
  import VaultLibraryBrowseLists from "$lib/components/vault/VaultLibraryBrowseLists.svelte";
  import VaultRootPicker from "$lib/components/vault/VaultRootPicker.svelte";
  import VaultTree from "$lib/components/vault/VaultTree.svelte";
  import { allFilterSpaces } from "$lib/config/vaultSpaces";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault, type LibraryBrowseMode } from "$lib/stores/vault.svelte";
  import { vaultFolderIcons } from "$lib/stores/vaultFolderIcons.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { formatShortcut } from "$lib/platform";
  import { canUseLocalVaultFilesystem } from "$lib/utils/vaultFilesystem";
  import { iconForSpace } from "$lib/utils/vaultSpaceIcons";

  let createOpen = $state(false);
  let filterOpen = $state(false);
  let listScrollEl = $state<HTMLElement | null>(null);

  const searching = $derived(vault.searchQuery.trim().length > 0);
  const folderIconMap = $derived(vaultFolderIcons.icons);
  const visibleSpaces = $derived(allFilterSpaces(vault.showSystemNotes));
  const spaceCounts = $derived(vault.spaceCountsMap);

  const browseModes: { id: LibraryBrowseMode; label: string }[] = [
    { id: "recent", label: "Recent" },
    { id: "folders", label: "Folders" },
    { id: "tags", label: "Tags" },
    { id: "kind", label: "Kind" },
  ];

  const filterActive = $derived(
    searching ||
      vault.libraryBrowseMode !== "recent" ||
      vault.activeSpaceFilter !== null ||
      vault.showAgentReviewFilter ||
      vault.showSystemNotes,
  );

  onMount(() => {
    let cancelled = false;
    (async () => {
      await vault.refreshVaultRoots();
      if (cancelled) return;
      await vault.refreshNotes();
      if (cancelled) return;
      if (vault.selectedPath) {
        await lmeWorkspace.openNote(vault.selectedPath, { activateMode: false });
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    void vault.labelByPathMap;
    untrack(() => lmeWorkspace.refreshNoteTitles());
  });

  function closeMenus() {
    createOpen = false;
    filterOpen = false;
  }

  function handleMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenus();
    }
  }

  function selectBrowseMode(mode: LibraryBrowseMode) {
    if (searching) void vault.runSearch("");
    vault.setLibraryBrowseMode(mode);
  }

  function clearFilters() {
    void vault.runSearch("");
    vault.setLibraryBrowseMode("recent");
    selectSpace(null);
    vault.setShowAgentReviewFilter(false);
    vault.setShowSystemNotes(false);
  }

  function selectSpace(spaceId: string | null) {
    vault.setActiveSpaceFilter(spaceId);
  }

  function onNotesSearch(value: string) {
    void vault.runSearch(value);
  }

  function handleListScroll(event: Event) {
    layout.setLibraryListScrollTop((event.currentTarget as HTMLDivElement).scrollTop);
  }

  $effect(() => {
    const el = listScrollEl;
    if (!el) return;
    const top = untrack(() => layout.libraryListScrollTop);
    void tick().then(() => {
      if (listScrollEl === el) el.scrollTop = top;
    });
  });
</script>

<svelte:window onclick={closeMenus} />

<aside class="lme-notes-explorer flex h-full min-h-0 w-full flex-col" aria-label="Notes">
  {#if vault.error}
    <p class="shrink-0 px-3 py-2 text-sm text-error-400">{vault.error}</p>
  {/if}

  <div
    class="min-h-0 flex-1 overflow-y-auto"
    bind:this={listScrollEl}
    onscroll={handleListScroll}
  >
    {#if searching}
      {#if vault.searchHits.length === 0}
        <p class="workshop-muted px-3 py-4 text-xs">No notes match.</p>
      {:else}
        <ul class="divide-y divide-surface-500/35 border-b border-surface-500/35">
          {#each vault.searchHits as hit (hit.note.path)}
            <li>
              <button
                type="button"
                class="flex w-full items-center gap-2 px-3 py-2 text-left transition hover:bg-surface-800/70 {vault.selectedPath ===
                hit.note.path
                  ? 'workshop-list-row-active'
                  : ''}"
                onclick={() => void lmeWorkspace.openNote(hit.note.path)}
              >
                <span class="min-w-0 flex-1">
                  <span class="block truncate text-sm font-medium text-surface-100">
                    {vault.labelByPathMap.get(hit.note.path) ??
                      vaultDisplayTitle(hit.note.title, hit.note.path)}
                  </span>
                  <span class="workshop-faint mt-0.5 block truncate font-mono text-[10px]">
                    {hit.note.path}
                  </span>
                </span>
                <VaultKindBadge kind={hit.note.kind} path={hit.note.path} compact />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if vault.libraryBrowseMode === "folders"}
      <VaultTree
        tree={vault.tree}
        selectedPath={vault.selectedPath}
        labelByPath={vault.labelByPathMap}
        activeSpaceFilter={vault.activeSpaceFilter}
        onSelect={(path) => void lmeWorkspace.openNote(path)}
        onMoveNote={(sourcePath, targetPrefix) => {
          void vault.moveNoteToFolder(sourcePath, targetPrefix);
        }}
      />
    {:else}
      <VaultLibraryBrowseLists onSelect={(path) => void lmeWorkspace.openNote(path)} />
    {/if}
  </div>

  <footer
    class="relative flex shrink-0 items-center gap-1 border-t border-surface-500/25 px-2 py-1.5"
  >
    <div class="min-w-0 flex-1">
      <VaultRootPicker compact quiet dropUp />
    </div>

    <div class="relative shrink-0">
      <button
        type="button"
        class="vault-dock-icon-btn"
        aria-haspopup="menu"
        aria-expanded={createOpen}
        aria-label="New note"
        title="New"
        onclick={(event) => {
          event.stopPropagation();
          filterOpen = false;
          createOpen = !createOpen;
        }}
      >
        <Plus size={16} strokeWidth={1.75} />
      </button>
      {#if createOpen}
        <div
          class="absolute bottom-full right-0 z-30 mb-1 min-w-[11rem] rounded-lg border border-surface-500/50 bg-surface-900 py-1 shadow-xl"
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

    <div class="relative shrink-0">
      <button
        type="button"
        class="vault-dock-icon-btn {filterActive ? 'vault-dock-icon-btn-active' : ''}"
        aria-haspopup="menu"
        aria-expanded={filterOpen}
        aria-label="Filter notes"
        title="Filter"
        onclick={(event) => {
          event.stopPropagation();
          createOpen = false;
          filterOpen = !filterOpen;
        }}
      >
        <SlidersHorizontal size={15} strokeWidth={1.75} />
      </button>
      {#if filterOpen}
        <div
          class="vault-notes-filter-menu absolute bottom-full right-0 z-30 mb-1 w-[min(17.5rem,calc(100vw-2rem))] rounded-lg border border-surface-500/50 bg-surface-900 py-2 shadow-xl"
          role="menu"
          tabindex="-1"
          onclick={(event) => event.stopPropagation()}
          onkeydown={handleMenuKeydown}
        >
          <div class="px-2.5 pb-2">
            <input
              class="input w-full text-xs"
              type="search"
              placeholder="Search notes…"
              value={vault.searchQuery}
              oninput={(event) =>
                onNotesSearch((event.currentTarget as HTMLInputElement).value)}
              onclick={(event) => event.stopPropagation()}
            />
          </div>

          <div
            class="vault-notes-filter-seg mx-2.5 mb-2 grid grid-cols-4 gap-0.5 rounded-md bg-surface-800/80 p-0.5"
            role="radiogroup"
            aria-label="Browse mode"
          >
            {#each browseModes as mode (mode.id)}
              <button
                type="button"
                role="menuitemradio"
                aria-checked={!searching && vault.libraryBrowseMode === mode.id}
                class="vault-notes-filter-seg-btn {!searching &&
                vault.libraryBrowseMode === mode.id
                  ? 'vault-notes-filter-seg-btn-active'
                  : ''}"
                onclick={() => selectBrowseMode(mode.id)}
              >
                {mode.label}
              </button>
            {/each}
          </div>

          <p class="px-3 pb-1 pt-0.5 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
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
              class="vault-menu-item w-full justify-between {vault.activeSpaceFilter === space.id
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
            Include
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

          {#if filterActive}
            <div class="my-1 border-t border-surface-500/35"></div>
            <button
              type="button"
              role="menuitem"
              class="vault-menu-item text-surface-400"
              onclick={clearFilters}
            >
              Clear filters
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </footer>
</aside>
