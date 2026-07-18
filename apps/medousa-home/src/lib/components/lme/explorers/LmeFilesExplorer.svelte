<script lang="ts">
  import { onMount } from "svelte";
  import ExternalFilesBrowser from "$lib/components/vault/ExternalFilesBrowser.svelte";
  import ExternalFileRow from "$lib/components/vault/ExternalFileRow.svelte";
  import VaultLibraryChrome from "$lib/components/vault/VaultLibraryChrome.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { lmeWorkspace } from "$lib/stores/lmeWorkspace.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { canPreviewAttachment } from "$lib/utils/vaultAttachments";
  import type { ExternalFileEntry } from "$lib/types/externalDesk";

  const externalHits = $derived(externalDesk.searchHitsList);
  const showFilesSearch = $derived(vault.searchQuery.trim().length > 0);
  const canLinkFiles = $derived(Boolean(vault.selectedPath));

  onMount(() => {
    if (externalDesk.pinnedRoots.length > 0) {
      void externalDesk.refreshAllRoots();
    }
  });

  function handleExternalSearch(query: string) {
    vault.searchQuery = query;
    externalDesk.setSearchQuery(query);
  }

  async function handleOpen(entry: ExternalFileEntry) {
    const attachment = externalDesk.attachmentForPath(entry.path);
    if (canPreviewAttachment(attachment)) {
      lmeWorkspace.openFile(entry.path);
      return;
    }
    externalDesk.selectExternalPath(entry.path);
    await openAttachmentPath(entry.path);
  }

  function handleLink(entry: ExternalFileEntry) {
    if (!vault.selectedPath) return;
    vault.linkExternalFile(entry.path);
  }
</script>

<aside class="lme-files-explorer flex h-full min-h-0 w-full flex-col" aria-label="Files">
  <VaultLibraryChrome
    showVaultChrome={false}
    hideLibraryTabs={true}
    onSearchExternal={handleExternalSearch}
  />

  {#if showFilesSearch}
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
                onOpen={handleOpen}
                onLink={handleLink}
              />
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {:else}
    <ExternalFilesBrowser onOpenFile={handleOpen} />
  {/if}
</aside>
