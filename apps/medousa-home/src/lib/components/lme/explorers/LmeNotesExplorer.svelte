<script lang="ts">
  import { onMount, untrack } from "svelte";
  import VaultTree from "$lib/components/vault/VaultTree.svelte";
  import VaultLibraryBrowseLists from "$lib/components/vault/VaultLibraryBrowseLists.svelte";
  import VaultLibraryChrome from "$lib/components/vault/VaultLibraryChrome.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { shouldShowGarageWizard } from "$lib/utils/garageOnboarding";

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
      if (cancelled) return;
      if (shouldShowGarageWizard() && !vault.selectedPath) {
        vault.openGarageWizard();
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
</script>

<aside class="lme-notes-explorer flex h-full min-h-0 w-full flex-col" aria-label="Notes">
  <VaultLibraryChrome showVaultChrome={true} hideLibraryTabs={true} />

  {#if vault.error}
    <p
      class="mx-2 mb-2 rounded-container-token border border-error-500/30 bg-error-500/10 px-2 py-1.5 text-xs text-error-300"
    >
      {vault.error}
    </p>
  {/if}

  {#if vault.libraryBrowseMode === "folders"}
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
</aside>
