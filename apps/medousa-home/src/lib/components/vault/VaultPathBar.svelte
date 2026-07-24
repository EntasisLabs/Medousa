<script lang="ts">
  import { ChevronRight } from "@lucide/svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { vaultPathCrumbs, type VaultPathCrumb } from "$lib/utils/formatVault";

  interface Props {
    path: string;
    title?: string | null;
  }

  let { path, title = null }: Props = $props();

  const crumbs = $derived(vaultPathCrumbs(path, title));

  function openFolder(crumb: VaultPathCrumb) {
    layout.openShellSidebarView("library");
    lmeWorkspace.setExplorerMode("notes");
    // Focus the space that owns this folder prefix (Inbox, Journal, …).
    vault.focusSpaceForPath(crumb.key, crumb.label);
  }
</script>

{#if crumbs.length > 0}
  <nav class="vault-path-bar" aria-label="Note path">
    <ol class="vault-path-list">
      {#each crumbs as crumb, index (crumb.key)}
        {#if index > 0}
          <li class="vault-path-sep" aria-hidden="true">
            <ChevronRight size={11} strokeWidth={2} />
          </li>
        {/if}
        <li class="vault-path-item min-w-0">
          {#if crumb.kind === "folder"}
            <button
              type="button"
              class="vault-path-crumb vault-path-crumb--folder truncate"
              title="Show in library — {crumb.label}"
              onclick={() => openFolder(crumb)}
            >
              {crumb.label}
            </button>
          {:else}
            <span
              class="vault-path-crumb vault-path-crumb--file truncate"
              title={path}
            >
              {crumb.label}
            </span>
          {/if}
        </li>
      {/each}
    </ol>
  </nav>
{/if}

<style>
  .vault-path-bar {
    min-width: 0;
    flex: 1 1 auto;
    max-width: 100%;
  }

  .vault-path-list {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.1rem;
    margin: 0;
    padding: 0;
    list-style: none;
    overflow: hidden;
  }

  .vault-path-sep {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    color: rgb(var(--color-surface-600));
    opacity: 0.85;
  }

  .vault-path-item {
    display: inline-flex;
    min-width: 0;
    max-width: 10rem;
    align-items: center;
  }

  .vault-path-item:last-child {
    max-width: 14rem;
    flex-shrink: 1;
  }

  .vault-path-crumb {
    min-width: 0;
    border: 0;
    background: transparent;
    padding: 0.1rem 0.2rem;
    border-radius: 0.25rem;
    font: inherit;
    font-size: 0.6875rem;
    line-height: 1.2;
    letter-spacing: 0.01em;
    color: rgb(var(--color-surface-500));
  }

  .vault-path-crumb--folder {
    cursor: pointer;
    transition: color 120ms ease, background-color 120ms ease;
  }

  .vault-path-crumb--folder:hover {
    color: rgb(var(--color-surface-200));
    background: rgb(var(--color-surface-800) / 0.55);
  }

  .vault-path-crumb--file {
    color: rgb(var(--color-surface-300));
    font-weight: 500;
  }
</style>
