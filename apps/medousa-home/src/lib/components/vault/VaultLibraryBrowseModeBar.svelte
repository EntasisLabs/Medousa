<script lang="ts">
  import {
    vault,
    type LibraryBrowseMode,
  } from "$lib/stores/vault.svelte";

  interface Props {
    /** Drop horizontal padding when nested in an already-padded parent. */
    flush?: boolean;
  }

  let { flush = false }: Props = $props();

  const modes: { id: LibraryBrowseMode; label: string }[] = [
    { id: "folders", label: "Folders" },
    { id: "tags", label: "Tags" },
    { id: "recent", label: "Recent" },
    { id: "kind", label: "Kind" },
  ];
</script>

<div
  class="vault-browse-mode-bar {flush ? 'vault-browse-mode-bar--flush' : ''}"
  role="tablist"
  aria-label="Library browse mode"
>
  {#each modes as mode (mode.id)}
    <button
      type="button"
      role="tab"
      aria-selected={vault.libraryBrowseMode === mode.id}
      class="vault-browse-mode-btn {vault.libraryBrowseMode === mode.id
        ? 'vault-browse-mode-btn-active'
        : ''}"
      onclick={() => vault.setLibraryBrowseMode(mode.id)}
    >
      {mode.label}
    </button>
  {/each}
</div>
