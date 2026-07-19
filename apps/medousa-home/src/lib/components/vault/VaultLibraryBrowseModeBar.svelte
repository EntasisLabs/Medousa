<script lang="ts">
  import {
    vault,
    type LibraryBrowseMode,
  } from "$lib/stores/vault.svelte";

  interface Props {
    /** Drop horizontal padding when nested in an already-padded parent. */
    flush?: boolean;
    /** Text chips — no pill chrome. Recent first for a short path. */
    quiet?: boolean;
  }

  let { flush = false, quiet = false }: Props = $props();

  const modes: { id: LibraryBrowseMode; label: string }[] = quiet
    ? [
        { id: "recent", label: "Recent" },
        { id: "folders", label: "Folders" },
        { id: "tags", label: "Tags" },
        { id: "kind", label: "Kind" },
      ]
    : [
        { id: "folders", label: "Folders" },
        { id: "tags", label: "Tags" },
        { id: "recent", label: "Recent" },
        { id: "kind", label: "Kind" },
      ];
</script>

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
      onclick={() => vault.setLibraryBrowseMode(mode.id)}
    >
      {mode.label}
    </button>
  {/each}
</div>
