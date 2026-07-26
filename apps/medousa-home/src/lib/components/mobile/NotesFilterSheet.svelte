<script lang="ts">
  import { Clock, FolderTree, Shapes, Tags } from "@lucide/svelte";
  import { selectableGroupSpaces } from "$lib/config/vaultSpaces";
  import { haptic } from "$lib/haptics";
  import { vault, type LibraryBrowseMode } from "$lib/stores/vault.svelte";
  import { iconForSpace } from "$lib/utils/vaultSpaceIcons";
  import { attachMobileSheetGestures } from "$lib/utils/mobileSheetGestures";
  import type { Component } from "svelte";

  export type NotesLibrarySection = "notes" | "presentations";

  interface Props {
    open: boolean;
    librarySection: NotesLibrarySection;
    onClose: () => void;
    onLibrarySection: (section: NotesLibrarySection) => void;
  }

  let { open, librarySection, onClose, onLibrarySection }: Props = $props();

  let sheetEl = $state<HTMLDivElement | null>(null);
  let headerEl = $state<HTMLElement | null>(null);

  const visibleSpaces = $derived(selectableGroupSpaces(vault.showSystemNotes));
  const counts = $derived(vault.spaceCountsMap);

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
      class="mobile-sheet mobile-sheet-tall"
      role="dialog"
      aria-label="Browse notes"
    >
      <header
        bind:this={headerEl}
        class="mobile-sheet-header mobile-activity-sheet-header scripts-workbench-sheet-header"
      >
        <div class="mobile-turn-sheet-grabber" aria-hidden="true"></div>
        <div class="flex w-full items-start justify-between gap-2">
          <div class="min-w-0">
            <h2 class="text-sm font-semibold text-surface-50">Browse</h2>
            <p class="workshop-faint mt-0.5 text-xs">Spaces, view, and library</p>
          </div>
          <button type="button" class="btn btn-sm variant-ghost-surface shrink-0" onclick={dismiss}>
            Done
          </button>
        </div>
      </header>

      <div class="mobile-you-scroll min-h-0 flex-1 space-y-6 overflow-y-auto px-4 py-4">
        <section>
          <h3 class="mobile-you-section-title">Library</h3>
          <div class="mt-2 flex gap-1 rounded-xl border border-surface-500/35 bg-surface-900/50 p-1">
            <button
              type="button"
              class="mobile-notes-filter-seg {librarySection === 'notes'
                ? 'mobile-notes-filter-seg-active'
                : ''}"
              aria-pressed={librarySection === "notes"}
              onclick={() => selectSection("notes")}
            >
              Notes
            </button>
            <button
              type="button"
              class="mobile-notes-filter-seg {librarySection === 'presentations'
                ? 'mobile-notes-filter-seg-active'
                : ''}"
              aria-pressed={librarySection === "presentations"}
              onclick={() => selectSection("presentations")}
            >
              Presentations
            </button>
          </div>
        </section>

        {#if librarySection === "notes"}
          <section>
            <h3 class="mobile-you-section-title">Spaces</h3>
            <div class="mt-2 flex flex-wrap gap-1.5" role="listbox" aria-label="Vault spaces">
              <button
                type="button"
                role="option"
                aria-selected={vault.activeSpaceFilter === null}
                class="mobile-notes-filter-chip {vault.activeSpaceFilter === null
                  ? 'mobile-notes-filter-chip-active'
                  : ''}"
                onclick={() => selectSpace(null)}
              >
                All
              </button>
              {#each visibleSpaces as space (space.id)}
                {@const Icon = iconForSpace(space.id)}
                {@const count = counts.get(space.id) ?? 0}
                <button
                  type="button"
                  role="option"
                  aria-selected={vault.activeSpaceFilter === space.id}
                  class="mobile-notes-filter-chip {vault.activeSpaceFilter === space.id
                    ? 'mobile-notes-filter-chip-active'
                    : ''}"
                  onclick={() => selectSpace(space.id)}
                >
                  <Icon size={12} strokeWidth={2} />
                  {space.label}
                  {#if count > 0}
                    <span class="workshop-faint tabular-nums">{count}</span>
                  {/if}
                </button>
              {/each}
            </div>
          </section>

          <section>
            <h3 class="mobile-you-section-title">View</h3>
            <ul class="mt-2 space-y-1">
              {#each browseModes as mode (mode.id)}
                {@const Icon = mode.Icon}
                <li>
                  <button
                    type="button"
                    class="mobile-notes-filter-row {vault.libraryBrowseMode === mode.id
                      ? 'mobile-notes-filter-row-active'
                      : ''}"
                    aria-pressed={vault.libraryBrowseMode === mode.id}
                    onclick={() => selectMode(mode.id)}
                  >
                    <Icon size={16} strokeWidth={1.75} class="shrink-0 text-surface-400" />
                    <span class="min-w-0 flex-1 text-left text-sm font-medium text-surface-100"
                      >{mode.label}</span
                    >
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {/if}
      </div>
    </div>
  </div>
{/if}
