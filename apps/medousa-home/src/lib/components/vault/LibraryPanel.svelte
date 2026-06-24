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
  import ExternalFileRow from "./ExternalFileRow.svelte";
  import VaultNewGroupDialog from "./VaultNewGroupDialog.svelte";
  import VaultSidebarCollapsedStrip from "./VaultSidebarCollapsedStrip.svelte";
  import VaultLibraryChrome from "./VaultLibraryChrome.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { canPreviewAttachment } from "$lib/utils/vaultAttachments";
  import type { ExternalFileEntry } from "$lib/types/externalDesk";
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
  const showFilesSearch = $derived(
    !showVaultChrome && vault.searchQuery.trim().length > 0,
  );
  const canLinkFiles = $derived(Boolean(vault.selectedPath));

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

  async function handleExternalOpen(entry: ExternalFileEntry) {
    externalDesk.selectExternalPath(entry.path);
    const attachment = externalDesk.attachmentForPath(entry.path);
    if (canPreviewAttachment(attachment)) {
      vault.previewAttachment(entry.path);
      return;
    }
    await openAttachmentPath(entry.path);
  }

  function handleLinkExternalHit(entry: ExternalFileEntry) {
    if (!vault.selectedPath) return;
    vault.linkExternalFile(entry.path);
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
        {:else if showFilesSearch}
          <div class="flex min-h-0 flex-1 flex-col overflow-y-auto p-2">
            <p class="mb-2 px-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
              Search results
            </p>
            {#if externalHits.length === 0}
              <p class="px-2 py-4 text-sm text-surface-500">No matches in pinned folders.</p>
            {:else}
              <ul class="space-y-0.5">
                {#each externalHits as entry (entry.path)}
                  <li>
                    <ExternalFileRow
                      {entry}
                      selected={externalDesk.selectedExternalPath === entry.path}
                      showLink={canLinkFiles}
                      onOpen={handleExternalOpen}
                      onLink={handleLinkExternalHit}
                    />
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
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
