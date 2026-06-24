<script lang="ts">
  import { onMount } from "svelte";
  import SplitPane from "$lib/components/layout/SplitPane.svelte";
  import { layout } from "$lib/stores/layout.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import VaultTree from "./VaultTree.svelte";
  import VaultEditor from "./VaultEditor.svelte";
  import VaultNewNoteDialog from "./VaultNewNoteDialog.svelte";
  import ExternalFilesBrowser from "./ExternalFilesBrowser.svelte";
  import VaultNewGroupDialog from "./VaultNewGroupDialog.svelte";
  import VaultSidebarCollapsedStrip from "./VaultSidebarCollapsedStrip.svelte";
  import VaultLibraryChrome from "./VaultLibraryChrome.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { shouldShowGarageWizard } from "$lib/utils/garageOnboarding";

  interface Props {
    visible: boolean;
    onOpenChat: () => void;
    onOpenWork: () => void;
    onSelectCard: (id: string) => void | Promise<void>;
  }

  let { visible, onOpenChat, onOpenWork, onSelectCard }: Props = $props();

  const externalHits = $derived(externalDesk.searchHitsList);
  const showVaultChrome = $derived(externalDesk.sidebarMode === "vault");
  const showExternalSearchResults = $derived(
    !showVaultChrome && vault.searchQuery.trim().length > 0 && externalHits.length > 0,
  );

  onMount(() => {
    (async () => {
      await vault.refreshVaultRoots();
      await vault.refreshNotes();
      if (vault.selectedPath) {
        await vault.openNote(vault.selectedPath);
      }
      if (externalDesk.sidebarMode === "files" && externalDesk.pinnedRoots.length > 0) {
        await externalDesk.refreshAllRoots();
      }
      if (shouldShowGarageWizard() && !vault.selectedPath) {
        vault.openGarageWizard();
      }
    })();
  });

  function handleExternalSearch(query: string) {
    vault.searchQuery = query;
    externalDesk.setSearchQuery(query);
  }

  async function handleExternalSearchHit(path: string) {
    externalDesk.selectExternalPath(path);
    await openAttachmentPath(path);
  }

  function handleLinkExternalHit(path: string) {
    if (!vault.selectedPath) return;
    vault.linkExternalFile(path);
  }
</script>

<section class="flex h-full min-w-0 flex-1 {visible ? '' : 'hidden'}">
  {#if layout.vaultSidebarCollapsed}
    <VaultSidebarCollapsedStrip onExpand={() => layout.setVaultSidebarCollapsed(false)} />
  {:else}
    <SplitPane
      width={layout.vaultTreeWidth}
      side="left"
      min={180}
      max={420}
      onResize={(width) => layout.setVaultTreeWidth(width)}
    >
      <aside
        class="workshop-drawer flex h-full w-full flex-col border-r-2"
        aria-label="Library browser"
      >
        <VaultLibraryChrome {showVaultChrome} onSearchExternal={handleExternalSearch} />

        {#if showExternalSearchResults}
          <div class="max-h-44 shrink-0 overflow-y-auto border-b border-surface-500/45 p-2 text-sm">
            <p class="mb-1 px-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
              Your files
            </p>
            <ul>
              {#each externalHits as entry (entry.path)}
                <li>
                  <div class="flex items-center gap-1 rounded-container-token px-2 py-1 hover:bg-surface-700/80">
                    <button
                      type="button"
                      class="min-w-0 flex-1 text-left"
                      onclick={() => void handleExternalSearchHit(entry.path)}
                    >
                      <span class="font-medium">{entry.name}</span>
                      <span class="workshop-faint block truncate">{entry.path}</span>
                    </button>
                    {#if vault.selectedPath}
                      <button
                        type="button"
                        class="btn btn-xs variant-soft-primary"
                        onclick={() => handleLinkExternalHit(entry.path)}
                      >
                        Link
                      </button>
                    {/if}
                  </div>
                </li>
              {/each}
            </ul>
          </div>
        {/if}

        {#if showVaultChrome}
          {#if vault.error}
            <p class="mx-2 mb-2 rounded-container-token border border-error-500/30 bg-error-500/10 px-2 py-1.5 text-xs text-error-300">
              {vault.error}
            </p>
          {/if}
          <VaultTree
            tree={vault.tree}
            selectedPath={vault.selectedPath}
            labelByPath={vault.labelByPathMap}
            activeSpaceFilter={vault.activeSpaceFilter}
            onSelect={(path) => vault.openNote(path)}
            onMoveNote={(sourcePath, targetPrefix) => {
              void vault.moveNoteToFolder(sourcePath, targetPrefix);
            }}
          />
        {:else}
          <ExternalFilesBrowser />
        {/if}
      </aside>
    </SplitPane>
  {/if}

  <VaultEditor
    visible={true}
    {onOpenChat}
    {onOpenWork}
    {onSelectCard}
  />
</section>

<VaultNewNoteDialog />
<VaultNewGroupDialog />
