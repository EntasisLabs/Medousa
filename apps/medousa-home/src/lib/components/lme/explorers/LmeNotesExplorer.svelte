<script lang="ts">
  import {
    Calendar,
    CalendarRange,
    FilePlus,
    FileText,
    FolderPlus,
    Plus,
    Search,
    X,
  } from "@lucide/svelte";
  import { onMount, tick, untrack } from "svelte";
  import BodyPortal from "$lib/components/ui/BodyPortal.svelte";
  import VaultGroupPicker from "$lib/components/vault/VaultGroupPicker.svelte";
  import VaultKindBadge from "$lib/components/vault/VaultKindBadge.svelte";
  import VaultLibraryBrowseLists from "$lib/components/vault/VaultLibraryBrowseLists.svelte";
  import VaultLibraryBrowseModeBar from "$lib/components/vault/VaultLibraryBrowseModeBar.svelte";
  import VaultRootPicker from "$lib/components/vault/VaultRootPicker.svelte";
  import VaultTree from "$lib/components/vault/VaultTree.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultDisplayTitle } from "$lib/utils/formatVault";
  import { formatShortcut } from "$lib/platform";
  import {
    placeDockPopover,
    type DockPopoverPlacement,
  } from "$lib/utils/dockPopoverPlace";
  import { portLmeDock } from "$lib/utils/lmeDockHost";
  import { ensureRailPopoverOpen } from "$lib/utils/railPopoverChrome";
  import { canUseLocalVaultFilesystem } from "$lib/utils/vaultFilesystem";

  let createOpen = $state(false);
  let createBtnEl = $state<HTMLButtonElement | null>(null);
  let createMenuEl = $state<HTMLDivElement | null>(null);
  let createPlacement = $state<DockPopoverPlacement | null>(null);
  let searchExpanded = $state(false);
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let listScrollEl = $state<HTMLElement | null>(null);

  const searching = $derived(vault.searchQuery.trim().length > 0);

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

  $effect(() => {
    if (searching && !searchExpanded) {
      searchExpanded = true;
    }
  });

  function placeCreateMenu() {
    if (!createBtnEl) return;
    // Prefer below when docked in the rail popover toolbar; above in the status bar.
    const inPopover = Boolean(createBtnEl.closest(".nav-rail-view-popover-dock-slot"));
    createPlacement = placeDockPopover(createBtnEl, {
      preferUp: !inPopover,
      width: 196,
      maxHeight: 320,
    });
  }

  function closeMenus() {
    createOpen = false;
    createPlacement = null;
  }

  function toggleCreateMenu(event: MouseEvent) {
    event.stopPropagation();
    if (createOpen) {
      closeMenus();
      return;
    }
    createOpen = true;
    requestAnimationFrame(placeCreateMenu);
  }

  function handleMenuKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      closeMenus();
    }
  }

  function onCreatePointerDown(event: PointerEvent) {
    if (!createOpen) return;
    const target = event.target as Node;
    if (createBtnEl?.contains(target) || createMenuEl?.contains(target)) return;
    closeMenus();
  }

  $effect(() => {
    if (!createOpen) return;
    window.addEventListener("pointerdown", onCreatePointerDown);
    window.addEventListener("resize", placeCreateMenu);
    return () => {
      window.removeEventListener("pointerdown", onCreatePointerDown);
      window.removeEventListener("resize", placeCreateMenu);
    };
  });

  async function openSearch() {
    closeMenus();
    await ensureRailPopoverOpen();
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

  <footer class="lme-side-rail-dock" use:portLmeDock>
    {#if searchExpanded}
      <div class="lme-dock-search-expand flex min-w-0 flex-1 items-center gap-1">
        <Search size={14} strokeWidth={1.75} class="shrink-0 text-surface-500" aria-hidden="true" />
        <input
          bind:this={searchInputEl}
          class="min-w-0 flex-1 border-0 bg-transparent text-[12px] text-surface-100 placeholder:text-surface-500 focus:outline-none focus:ring-0"
          type="search"
          placeholder="Search notes…"
          value={vault.searchQuery}
          oninput={(event) =>
            onNotesSearch((event.currentTarget as HTMLInputElement).value)}
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
      <div
        class="lme-dock-chrome-secondary lme-dock-chrome-secondary--crumb flex min-w-0 items-center gap-0.5"
      >
        <VaultRootPicker compact quiet dropUp />
        <span
          class="nav-rail-dock-crumb-sep shrink-0 px-px text-[11px] font-medium leading-none text-surface-500"
          aria-hidden="true"
        >
          /
        </span>
        <VaultGroupPicker dropUp />
      </div>
      <!-- Push action cluster toward `>` once the bar extends. -->
      <div
        class="lme-dock-chrome-secondary lme-dock-chrome-secondary--spacer min-w-1 flex-1"
      ></div>

      <div class="relative shrink-0">
        <button
          bind:this={createBtnEl}
          type="button"
          class="vault-dock-icon-btn"
          aria-haspopup="menu"
          aria-expanded={createOpen}
          aria-label="New note"
          title="New"
          onclick={toggleCreateMenu}
        >
          <Plus size={16} strokeWidth={1.75} />
        </button>
      </div>
      {#if createOpen && createPlacement}
        <BodyPortal>
          <div
            bind:this={createMenuEl}
            class="vault-dock-popover"
            role="menu"
            tabindex="-1"
            style:left="{createPlacement.left}px"
            style:top="{createPlacement.top}px"
            style:width="{createPlacement.width}px"
            style:max-height="{createPlacement.maxHeight}px"
            style:transform={createPlacement.transform}
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
              <div class="vault-dock-popover__sep"></div>
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
        </BodyPortal>
      {/if}
      <div class="lme-dock-chrome-secondary shrink-0">
        <VaultLibraryBrowseModeBar icons flush rail />
      </div>

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
  </footer>
</aside>
