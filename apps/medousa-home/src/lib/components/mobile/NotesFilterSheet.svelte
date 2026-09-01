<script lang="ts">
  import {
    Check,
    Clock,
    FolderOpen,
    FolderTree,
    LayoutGrid,
    LoaderCircle,
    Shapes,
    Tags,
  } from "@lucide/svelte";
  import { selectableGroupSpaces } from "$lib/config/vaultSpaces";
  import { haptic } from "$lib/haptics";
  import { vault, type LibraryBrowseMode } from "$lib/stores/vault.svelte";
  import { iconForSpace } from "$lib/utils/vaultSpaceIcons";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
  import type { Component } from "svelte";

  export type NotesLibrarySection = "notes" | "artifacts";

  interface Props {
    open: boolean;
    librarySection: NotesLibrarySection;
    onClose: () => void;
    onLibrarySection: (section: NotesLibrarySection) => void;
  }

  let { open, librarySection, onClose, onLibrarySection }: Props = $props();

  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);
  let switchingVaultId = $state<string | null>(null);
  let vaultSwitchError = $state<string | null>(null);

  const visibleSpaces = $derived(selectableGroupSpaces(vault.showSystemNotes));
  const counts = $derived(vault.spaceCountsMap);
  const showVaultPicker = $derived(
    !vault.vaultRootsUnavailable && vault.vaultRoots.length > 1,
  );

  const browseModes: { id: LibraryBrowseMode; label: string; Icon: Component }[] = [
    { id: "recent", label: "Recent", Icon: Clock },
    { id: "folders", label: "Folders", Icon: FolderTree },
    { id: "tags", label: "Tags", Icon: Tags },
    { id: "kind", label: "Kind", Icon: Shapes },
  ];

  function dismiss() {
    haptic("light");
    onClose();
  }

  function selectSpace(spaceId: string | null) {
    haptic("light");
    vault.setActiveSpaceFilter(spaceId);
  }

  function selectMode(mode: LibraryBrowseMode) {
    haptic("light");
    if (vault.searchQuery.trim()) void vault.runSearch("");
    vault.setLibraryBrowseMode(mode);
    if (librarySection !== "notes") onLibrarySection("notes");
  }

  function selectSection(section: NotesLibrarySection) {
    haptic("light");
    onLibrarySection(section);
  }

  async function selectVaultRoot(rootId: string) {
    if (switchingVaultId || rootId === vault.activeVaultRootId) return;
    haptic("light");
    switchingVaultId = rootId;
    vaultSwitchError = null;
    try {
      await vault.switchVaultRoot(rootId);
    } catch (error) {
      vaultSwitchError = error instanceof Error ? error.message : String(error);
    } finally {
      switchingVaultId = null;
    }
  }

  $effect(() => {
    if (!open || !sheetEl) return;
    return attachMobileSheetGestures(sheetEl, headerEl, { onDismiss: dismiss });
  });
</script>

{#if open}
  <div
    class="mobile-sheet-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) dismiss();
    }}
  >
    <div
      bind:this={sheetEl}
      class="mobile-sheet mobile-turn-sheet mobile-notes-filter-sheet"
      role="dialog"
      aria-label="Browse notes"
    >
      <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
      <header bind:this={headerEl} class="mobile-turn-sheet-header">
        <span class="mobile-turn-sheet-header-spacer" aria-hidden="true"></span>
        <h2 class="mobile-turn-sheet-title">Browse</h2>
        <button type="button" class="mobile-sheet-done" onclick={dismiss}>Done</button>
      </header>

      <div class="mobile-turn-sheet-body mobile-notes-filter-body">
        {#if librarySection === "notes" && showVaultPicker}
          <section>
            <h3 class="mobile-notes-filter-section-title">Vault</h3>
            <div
              class="mobile-turn-sheet-group mobile-notes-vault-group"
              role="listbox"
              aria-label="Vault folders"
              aria-busy={switchingVaultId !== null}
            >
              {#each vault.vaultRoots as root, index (root.id)}
                {@const selected = root.id === vault.activeVaultRootId}
                <button
                  type="button"
                  role="option"
                  aria-selected={selected}
                  disabled={switchingVaultId !== null}
                  class="mobile-turn-sheet-row mobile-notes-vault-row {index > 0
                    ? 'mobile-turn-sheet-row-divider'
                    : ''} {selected ? 'mobile-notes-vault-row-active' : ''}"
                  onclick={() => void selectVaultRoot(root.id)}
                >
                  <span class="mobile-notes-view-icon">
                    <FolderOpen size={15} strokeWidth={1.75} />
                  </span>
                  <span class="mobile-turn-sheet-row-copy">
                    <span class="mobile-turn-sheet-row-title">{root.label}</span>
                    {#if root.isObsidian}
                      <span class="mobile-turn-sheet-row-subtitle">Obsidian vault</span>
                    {/if}
                  </span>
                  {#if switchingVaultId === root.id}
                    <LoaderCircle
                      size={15}
                      strokeWidth={1.9}
                      class="mobile-notes-vault-spinner"
                    />
                  {:else if selected}
                    <Check size={16} strokeWidth={2.2} class="mobile-turn-sheet-row-check" />
                  {/if}
                </button>
              {/each}
            </div>
            {#if vaultSwitchError}
              <p class="mobile-notes-vault-error">{vaultSwitchError}</p>
            {/if}
          </section>
        {/if}

        <section>
          <h3 class="mobile-notes-filter-section-title">Library</h3>
          <div class="mobile-notes-library-switch" role="tablist" aria-label="Library section">
            <button
              type="button"
              role="tab"
              class="mobile-notes-library-option {librarySection === 'notes'
                ? 'mobile-notes-library-option-active'
                : ''}"
              aria-selected={librarySection === "notes"}
              onclick={() => selectSection("notes")}
            >
              Notes
            </button>
            <button
              type="button"
              role="tab"
              class="mobile-notes-library-option {librarySection === 'artifacts'
                ? 'mobile-notes-library-option-active'
                : ''}"
              aria-selected={librarySection === "artifacts"}
              onclick={() => selectSection("artifacts")}
            >
              Artifacts
            </button>
          </div>
        </section>

        {#if librarySection === "notes"}
          <section>
            <h3 class="mobile-notes-filter-section-title">Spaces</h3>
            <div class="mobile-notes-space-options" role="listbox" aria-label="Vault spaces">
              <button
                type="button"
                role="option"
                aria-selected={vault.activeSpaceFilter === null}
                class="mobile-notes-space-option {vault.activeSpaceFilter === null
                  ? 'mobile-notes-space-option-active'
                  : ''}"
                onclick={() => selectSpace(null)}
              >
                <LayoutGrid size={13} strokeWidth={1.8} />
                <span>All</span>
                {#if vault.activeSpaceFilter === null}
                  <Check size={12} strokeWidth={2.2} class="mobile-notes-space-check" />
                {/if}
              </button>
              {#each visibleSpaces as space (space.id)}
                {@const Icon = iconForSpace(space.id)}
                {@const count = counts.get(space.id) ?? 0}
                <button
                  type="button"
                  role="option"
                  aria-selected={vault.activeSpaceFilter === space.id}
                  class="mobile-notes-space-option {vault.activeSpaceFilter === space.id
                    ? 'mobile-notes-space-option-active'
                    : ''}"
                  onclick={() => selectSpace(space.id)}
                >
                  <Icon size={13} strokeWidth={1.8} />
                  <span>{space.label}</span>
                  {#if count > 0}
                    <span class="mobile-notes-space-count">{count}</span>
                  {/if}
                  {#if vault.activeSpaceFilter === space.id}
                    <Check size={12} strokeWidth={2.2} class="mobile-notes-space-check" />
                  {/if}
                </button>
              {/each}
            </div>
          </section>

          <section>
            <h3 class="mobile-notes-filter-section-title">View</h3>
            <div
              class="mobile-turn-sheet-group mobile-notes-view-group"
              role="listbox"
              aria-label="Library view"
            >
              {#each browseModes as mode, index (mode.id)}
                {@const Icon = mode.Icon}
                <button
                  type="button"
                  role="option"
                  aria-selected={vault.libraryBrowseMode === mode.id}
                  class="mobile-turn-sheet-row mobile-notes-view-row {index > 0
                    ? 'mobile-turn-sheet-row-divider'
                    : ''} {vault.libraryBrowseMode === mode.id
                    ? 'mobile-notes-view-row-active'
                    : ''}"
                  onclick={() => selectMode(mode.id)}
                >
                  <span class="mobile-notes-view-icon">
                    <Icon size={15} strokeWidth={1.75} />
                  </span>
                  <span class="mobile-turn-sheet-row-copy">
                    <span class="mobile-turn-sheet-row-title">{mode.label}</span>
                  </span>
                  {#if vault.libraryBrowseMode === mode.id}
                    <Check size={16} strokeWidth={2.2} class="mobile-turn-sheet-row-check" />
                  {/if}
                </button>
              {/each}
            </div>
          </section>
        {/if}
      </div>
    </div>
  </div>
{/if}
