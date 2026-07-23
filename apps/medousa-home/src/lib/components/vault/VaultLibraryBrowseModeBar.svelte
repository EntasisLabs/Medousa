<script lang="ts">
  import { Clock, FolderTree, Shapes, Tags } from "@lucide/svelte";
  import {
    vault,
    type LibraryBrowseMode,
  } from "$lib/stores/vault.svelte";
  import type { Component } from "svelte";

  interface Props {
    /** Drop horizontal padding when nested in an already-padded parent. */
    flush?: boolean;
    /** Text chips — no pill chrome. Recent first for a short path. */
    quiet?: boolean;
    /** Icon buttons with tooltips (dock / chrome). */
    icons?: boolean;
    /** Rail popover dock: only the short verb set (Recent + Tags). */
    rail?: boolean;
  }

  let { flush = false, quiet = false, icons = false, rail = false }: Props = $props();

  const allModes: {
    id: LibraryBrowseMode;
    label: string;
    Icon: Component;
  }[] = [
    { id: "recent", label: "Recent", Icon: Clock },
    { id: "folders", label: "Folders", Icon: FolderTree },
    { id: "tags", label: "Tags", Icon: Tags },
    { id: "kind", label: "Kind", Icon: Shapes },
  ];

  const modes = $derived(
    rail
      ? allModes.filter((mode) => mode.id === "recent" || mode.id === "tags")
      : allModes,
  );

  function selectMode(mode: LibraryBrowseMode) {
    if (vault.searchQuery.trim()) void vault.runSearch("");
    vault.setLibraryBrowseMode(mode);
  }
</script>

{#if icons}
  <div
    class="vault-browse-mode-icons {flush ? 'vault-browse-mode-icons--flush' : ''}"
    role="tablist"
    aria-label="Library browse mode"
  >
    {#each modes as mode (mode.id)}
      {@const Icon = mode.Icon}
      <button
        type="button"
        role="tab"
        aria-selected={vault.libraryBrowseMode === mode.id && !vault.searchQuery.trim()}
        class="vault-dock-icon-btn {vault.libraryBrowseMode === mode.id &&
        !vault.searchQuery.trim()
          ? 'vault-dock-icon-btn-active'
          : ''}"
        title={mode.label}
        aria-label={mode.label}
        onclick={() => selectMode(mode.id)}
      >
        <Icon size={15} strokeWidth={1.75} />
      </button>
    {/each}
  </div>
{:else}
  <div
    class="vault-browse-mode-bar {flush ? 'vault-browse-mode-bar--flush' : ''} {quiet
      ? 'vault-browse-mode-bar--quiet'
      : ''}"
    role="tablist"
    aria-label="Library browse mode"
  >
    {#each modes as mode (mode.id)}
      <button
        type="button"
        role="tab"
        aria-selected={vault.libraryBrowseMode === mode.id}
        class="vault-browse-mode-btn {quiet ? 'vault-browse-mode-btn--quiet' : ''} {vault.libraryBrowseMode ===
        mode.id
          ? quiet
            ? 'vault-browse-mode-btn-quiet-active'
            : 'vault-browse-mode-btn-active'
          : ''}"
        onclick={() => selectMode(mode.id)}
      >
        {mode.label}
      </button>
    {/each}
  </div>
{/if}
