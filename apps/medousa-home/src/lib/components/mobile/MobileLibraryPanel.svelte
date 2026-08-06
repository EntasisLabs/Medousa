<script lang="ts">
  import { onMount, tick } from "svelte";
  import ArtifactFullscreen from "$lib/components/chat/ArtifactFullscreen.svelte";
  import ArtifactLibraryList from "$lib/components/artifacts/ArtifactLibraryList.svelte";
  import VaultTree from "$lib/components/vault/VaultTree.svelte";
  import VaultLibraryBrowseLists from "$lib/components/vault/VaultLibraryBrowseLists.svelte";
  import VaultEditor from "$lib/components/vault/VaultEditor.svelte";
  import VaultKindBadge from "$lib/components/vault/VaultKindBadge.svelte";
  import VaultNewNoteDialog from "$lib/components/vault/VaultNewNoteDialog.svelte";
  import NotesFilterSheet from "$lib/components/mobile/NotesFilterSheet.svelte";
  import { getSpaceById } from "$lib/config/vaultSpaces";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { artifacts } from "$lib/stores/artifacts.svelte";
  import { chat } from "$lib/stores/chat.svelte";
  import {
    bindVaultLongPress,
    handleVaultNoteContextMenuEvent,
    shouldSuppressVaultContextMenuClick,
  } from "$lib/utils/vaultContextMenuEvents";
  import { registerMobileBackHandler } from "$lib/mobileNavigation";
  import { artifactSummaryToUi } from "$lib/types/artifact";
  import type { ArtifactSummary } from "$lib/types/artifact";
  import type { UiArtifact } from "$lib/types/chat";

  type LibraryTab = "notes" | "presentations";

  interface Props {
    visible: boolean;
    onOpenChat?: () => void | Promise<void>;
  }

  let { visible, onOpenChat }: Props = $props();

  let listScrollEl = $state<HTMLDivElement | null>(null);
  let notesSearchEl = $state<HTMLInputElement | null>(null);
  let presentationsSearchEl = $state<HTMLInputElement | null>(null);
  let libraryTab = $state<LibraryTab>("notes");
  let searchOpen = $state(false);
  let filterSheetOpen = $state(false);
  let presentationArtifact = $state<ArtifactSummary | null>(null);
  let presentationFullscreenOpen = $state(false);

  const view = $derived(layout.libraryView);
  const presentationUiArtifact = $derived.by((): UiArtifact | null =>
    presentationArtifact ? artifactSummaryToUi(presentationArtifact) : null,
  );

  const listTitle = $derived(libraryTab === "presentations" ? "Presentations" : "Notes");

  const browseModeLabel = $derived.by(() => {
    switch (vault.libraryBrowseMode) {
      case "recent":
        return "Recent";
      case "tags":
        return "Tags";
      case "kind":
        return "Kind";
      default:
        return "Folders";
    }
  });

  const spaceFilterLabel = $derived.by(() => {
    if (!vault.activeSpaceFilter) return null;
    return getSpaceById(vault.activeSpaceFilter)?.label ?? vault.activeSpaceFilter;
  });

  const filterChipLabel = $derived.by(() => {
    if (libraryTab === "presentations") return "Presentations";
    const parts: string[] = [];
    if (spaceFilterLabel) parts.push(spaceFilterLabel);
    if (vault.libraryBrowseMode !== "folders") parts.push(browseModeLabel);
    return parts.length > 0 ? parts.join(" · ") : null;
  });

  const showSearchField = $derived(
    searchOpen ||
      (libraryTab === "notes" && Boolean(vault.searchQuery.trim())) ||
      (libraryTab === "presentations" && Boolean(artifacts.searchQuery.trim())),
  );

  onMount(() => {
    const onSearchFocus = () => {
      searchOpen = true;
      void tick().then(() => {
        const el = libraryTab === "presentations" ? presentationsSearchEl : notesSearchEl;
        el?.focus();
        el?.select();
      });
    };
    const onFilter = () => {
      filterSheetOpen = true;
    };
    window.addEventListener("medousa-mobile-notes-search-focus", onSearchFocus);
    window.addEventListener("medousa-mobile-notes-filter", onFilter);

    void (async () => {
      await vault.refreshNotes();
      if (vault.selectedPath && !vault.content) {
        await vault.openNote(vault.selectedPath);
        if (layout.libraryView === "reader") {
          vault.enterPreviewMode();
        }
      }
    })();

    return () => {
      window.removeEventListener("medousa-mobile-notes-search-focus", onSearchFocus);
      window.removeEventListener("medousa-mobile-notes-filter", onFilter);
    };
  });

  $effect(() => {
    if (libraryTab === "presentations") {
      void artifacts.refresh();
    }
  });

  $effect(() => {
    if (!visible || view !== "list" || !listScrollEl) return;
    void tick().then(() => {
      if (listScrollEl) {
        listScrollEl.scrollTop = layout.libraryListScrollTop;
      }
    });
  });

  function handleSearchInput(event: Event) {
    const value = (event.currentTarget as HTMLInputElement).value;
    void vault.runSearch(value);
  }

  function clearNotesSearch() {
    void vault.runSearch("");
    searchOpen = false;
  }

  function clearPresentationsSearch() {
    artifacts.setSearchQuery("");
    searchOpen = false;
  }

  function handleListScroll(event: Event) {
    layout.setLibraryListScrollTop((event.currentTarget as HTMLDivElement).scrollTop);
  }

  async function openNote(path: string, event?: MouseEvent) {
    if (shouldSuppressVaultContextMenuClick()) return;
    if (!vault.applyRailSelection(path, event)) return;
    await vault.openNote(path);
    vault.enterPreviewMode();
    layout.setLibraryView("reader");
  }

  function backToList() {
    layout.setLibraryView("list");
  }

  $effect(() => {
    if (!visible) return;
    return registerMobileBackHandler(() => {
      if (filterSheetOpen) {
        filterSheetOpen = false;
        return true;
      }
      if (searchOpen || vault.searchQuery.trim() || artifacts.searchQuery.trim()) {
        clearNotesSearch();
        clearPresentationsSearch();
        return true;
      }
      if (libraryTab === "presentations" && presentationFullscreenOpen) {
        presentationFullscreenOpen = false;
        return true;
      }
      if (libraryTab === "presentations") {
        libraryTab = "notes";
        return true;
      }
      if (libraryTab === "notes" && layout.libraryView === "list") return false;
      if (libraryTab === "notes") {
        backToList();
        return true;
      }
      return false;
    });
  });

  const saveWhisper = $derived(vault.saveWhisper());

  function openPresentation(artifact: ArtifactSummary) {
    presentationArtifact = artifact;
    presentationFullscreenOpen = true;
  }

  async function openPresentationChat(artifact: ArtifactSummary) {
    chat.sessionId = artifact.session_id;
    await onOpenChat?.();
  }
</script>

<section class="flex h-full min-h-0 min-w-0 flex-1 flex-col {visible ? '' : 'hidden'}">
  {#if view === "reader" && libraryTab === "notes"}
    {#if saveWhisper}
      <p class="px-4 py-1 text-xs text-content-tertiary">{saveWhisper}</p>
    {/if}
    <VaultEditor visible={true} mobile={true} />
  {:else}
    <header class="mobile-notes-header px-4 pb-2">
      <h1 class="text-lg font-semibold tracking-tight text-surface-50">{listTitle}</h1>
      {#if filterChipLabel}
        <button
          type="button"
          class="mobile-notes-active-filter"
          onclick={() => (filterSheetOpen = true)}
        >
          {filterChipLabel}
        </button>
      {/if}
    </header>

    {#if showSearchField}
      <div class="shrink-0 border-b border-surface-500/30 px-3 pb-3">
        {#if libraryTab === "notes"}
          <div class="flex items-center gap-2">
            <input
              bind:this={notesSearchEl}
              class="input min-w-0 flex-1 text-sm"
              type="search"
              placeholder="Search notes…"
              value={vault.searchQuery}
              oninput={handleSearchInput}
              onkeydown={(event) => {
                if (event.key === "Escape") clearNotesSearch();
              }}
            />
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface shrink-0"
              onclick={clearNotesSearch}
            >
              Cancel
            </button>
          </div>
        {:else}
          <div class="flex items-center gap-2">
            <input
              bind:this={presentationsSearchEl}
              class="input min-w-0 flex-1 text-sm"
              type="search"
              placeholder="Filter presentations…"
              value={artifacts.searchQuery}
              oninput={(event) => artifacts.setSearchQuery(event.currentTarget.value)}
              onkeydown={(event) => {
                if (event.key === "Escape") clearPresentationsSearch();
              }}
            />
            <button
              type="button"
              class="btn btn-sm variant-ghost-surface shrink-0"
              onclick={clearPresentationsSearch}
            >
              Cancel
            </button>
          </div>
        {/if}
      </div>
    {/if}

    {#if libraryTab === "notes"}
      <div
        bind:this={listScrollEl}
        class="mobile-you-scroll min-h-0 flex-1 overflow-y-auto"
        onscroll={handleListScroll}
      >
        {#if vault.searchHits.length > 0}
          <ul class="border-b border-surface-500/40 p-2">
            {#each vault.searchHits as hit (hit.note.path)}
              <li>
                <button
                  type="button"
                  class="mobile-you-row flex w-full items-center gap-2 text-left"
                  onclick={() => void openNote(hit.note.path)}
                  oncontextmenu={(event) =>
                    handleVaultNoteContextMenuEvent(hit.note.path, event)}
                  use:bindVaultLongPress={() => hit.note.path}
                >
                  <span class="min-w-0 flex-1">
                    <span class="font-medium text-surface-100">{hit.note.title}</span>
                    <span class="workshop-faint block truncate text-xs">{hit.note.path}</span>
                  </span>
                  <VaultKindBadge kind={hit.note.kind} path={hit.note.path} compact />
                </button>
              </li>
            {/each}
          </ul>
        {/if}

        {#if vault.libraryBrowseMode === "folders"}
          <VaultTree
            tree={vault.tree}
            selectedPath={vault.selectedPath}
            labelByPath={vault.labelByPath()}
            activeSpaceFilter={vault.activeSpaceFilter}
            revealSelected={false}
            onSelect={openNote}
          />
        {:else}
          <VaultLibraryBrowseLists onSelect={openNote} />
        {/if}
      </div>
    {:else}
      <div class="flex min-h-0 flex-1 flex-col">
        {#if artifacts.error}
          <p
            class="mx-3 mt-3 rounded-container-token border border-error-500/30 bg-error-500/10 px-3 py-2 text-xs text-content-error"
          >
            {artifacts.error}
          </p>
        {/if}
        {#if artifacts.loading}
          <p class="px-3 py-4 text-sm text-content-quiet">Loading presentations…</p>
        {:else}
          <ArtifactLibraryList
            artifacts={artifacts.filteredArtifacts}
            selectedArtifactId={presentationArtifact?.artifact_id ?? null}
            sessionTitle={(sessionId) => artifacts.sessionTitle(sessionId)}
            onSelect={(artifactId) => {
              const match = artifacts.filteredArtifacts.find(
                (artifact) => artifact.artifact_id === artifactId,
              );
              if (match) openPresentation(match);
            }}
            onOpenChat={onOpenChat ? openPresentationChat : undefined}
          />
        {/if}
      </div>
    {/if}
  {/if}
</section>

{#if presentationUiArtifact && presentationArtifact}
  <ArtifactFullscreen
    open={presentationFullscreenOpen}
    sessionId={presentationArtifact.session_id}
    artifact={presentationUiArtifact}
    onClose={() => {
      presentationFullscreenOpen = false;
    }}
  />
{/if}

<NotesFilterSheet
  open={filterSheetOpen}
  librarySection={libraryTab}
  onClose={() => (filterSheetOpen = false)}
  onLibrarySection={(section) => {
    libraryTab = section;
  }}
/>

<VaultNewNoteDialog />
