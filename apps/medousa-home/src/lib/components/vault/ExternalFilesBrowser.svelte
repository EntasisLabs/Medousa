<script lang="ts">
  import {
    ChevronDown,
    ChevronRight,
    ChevronUp,
    Ellipsis,
    FolderOpen,
    RefreshCw,
    Unlink,
  } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { canPreviewAttachment } from "$lib/utils/vaultAttachments";
  import type { ExternalFileEntry, PinnedRoot } from "$lib/types/externalDesk";
  import ExternalFileRow from "./ExternalFileRow.svelte";
  import { isCoLocatedWorkshop, vaultPinFolderRemoteHint } from "$lib/utils/workshopLocality";
  import { hostComputerPhrase, onThisHostPhrase } from "$lib/platformCopy";

  interface Props {
    compact?: boolean;
    /** When set (LME), open goes through workspace tabs instead of pane preview only. */
    onOpenFile?: (entry: ExternalFileEntry) => void | Promise<void>;
  }

  let { compact = false, onOpenFile }: Props = $props();

  const RECENT_PEEK = $derived(compact ? 4 : 6);

  const canLink = $derived(Boolean(vault.selectedPath));
  const coLocated = $derived(isCoLocatedWorkshop());
  let openRootMenu = $state<string | null>(null);

  function visibleFiles(rootPath: string): ExternalFileEntry[] {
    return (externalDesk.entriesByRoot[rootPath] ?? []).filter((entry) => !entry.is_dir);
  }

  function sortedFiles(rootPath: string): ExternalFileEntry[] {
    return [...visibleFiles(rootPath)].sort(
      (a, b) =>
        new Date(b.modified_at_utc).getTime() - new Date(a.modified_at_utc).getTime(),
    );
  }

  function isExpanded(rootId: string): boolean {
    return externalDesk.isPinExpanded(rootId);
  }

  function togglePin(rootId: string) {
    externalDesk.togglePinExpanded(rootId);
  }

  function showAll(rootId: string): boolean {
    return externalDesk.isShowAll(rootId);
  }

  function setShowAll(rootId: string, value: boolean) {
    externalDesk.setShowAll(rootId, value);
  }

  function filesToShow(root: PinnedRoot): { entries: ExternalFileEntry[]; total: number } {
    const all = sortedFiles(root.path);
    if (showAll(root.id)) {
      return { entries: all, total: all.length };
    }
    return { entries: all.slice(0, RECENT_PEEK), total: all.length };
  }

  async function handleOpen(entry: ExternalFileEntry) {
    if (onOpenFile) {
      await onOpenFile(entry);
      return;
    }
    externalDesk.selectExternalPath(entry.path);
    const attachment = externalDesk.attachmentForPath(entry.path);
    if (canPreviewAttachment(attachment)) {
      vault.previewAttachment(entry.path, "pane");
      return;
    }
    await openAttachmentPath(entry.path);
  }

  function handleLink(entry: ExternalFileEntry) {
    if (!vault.selectedPath) return;
    vault.linkExternalFile(entry.path);
  }

  async function handleRefreshRoot(root: PinnedRoot) {
    await externalDesk.refreshRoot(root.path);
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  {#if externalDesk.error}
    <p class="border-b border-error-500/30 bg-error-500/10 px-3 py-2 text-xs text-content-error">
      {externalDesk.error}
    </p>
  {/if}

  {#if !coLocated}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
      <FolderOpen size={28} strokeWidth={1.5} class="text-content-quiet" />
      <div class="max-w-xs space-y-1">
        <p class="text-sm font-medium text-surface-100">Your files stay {onThisHostPhrase()}</p>
        <p class="text-xs leading-relaxed text-content-quiet">
          {vaultPinFolderRemoteHint()}
        </p>
      </div>
    </div>
  {:else if externalDesk.pinnedRoots.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
      <FolderOpen size={28} strokeWidth={1.5} class="text-content-quiet" />
      <div class="max-w-xs space-y-1">
        <p class="text-sm font-medium text-surface-100">
          Add folders from {hostComputerPhrase()}
        </p>
        <p class="text-xs leading-relaxed text-content-quiet">
          Folders you add show up here. Open a file, or link it into a note.
        </p>
      </div>
    </div>
  {:else}
    <div class="vault-external-groups min-h-0 flex-1 overflow-y-auto">
      {#each externalDesk.pinnedRoots as root (root.id)}
        {@const expanded = isExpanded(root.id)}
        {@const { entries, total } = filesToShow(root)}
        <section class="vault-external-group">
          <div class="vault-external-group-header">
            <button
              type="button"
              class="vault-external-pin-row"
              aria-expanded={expanded}
              title={root.path}
              onclick={() => togglePin(root.id)}
            >
              <span class="vault-external-pin-chevron">
                {#if expanded}
                  <ChevronDown size={13} strokeWidth={1.8} />
                {:else}
                  <ChevronRight size={13} strokeWidth={1.8} />
                {/if}
              </span>
              <FolderOpen size={14} strokeWidth={1.75} class="vault-external-pin-icon" />
              <span class="vault-external-pin-label">{root.label}</span>
              {#if total > 0}
                <span class="vault-external-pin-count">{total}</span>
              {/if}
            </button>
            <OverflowMenu
              open={openRootMenu === root.id}
              onOpenChange={(open) => (openRootMenu = open ? root.id : null)}
              align="right"
              panelWidth={176}
              panelClass="vault-external-folder-menu"
            >
              {#snippet trigger({ open, toggle })}
                <button
                  type="button"
                  class="vault-external-group-more {open ? 'vault-external-group-more-open' : ''}"
                  aria-label="Folder actions for {root.label}"
                  title="Folder actions"
                  aria-haspopup="menu"
                  aria-expanded={open}
                  onclick={toggle}
                >
                  <Ellipsis size={14} strokeWidth={1.8} />
                </button>
              {/snippet}
              <button
                type="button"
                role="menuitem"
                class="vault-external-folder-menu-item"
                disabled={externalDesk.loadingRoot === root.path}
                onclick={() => {
                  openRootMenu = null;
                  void handleRefreshRoot(root);
                }}
              >
                <RefreshCw size={13} strokeWidth={1.8} />
                <span>Refresh folder</span>
              </button>
              <button
                type="button"
                role="menuitem"
                class="vault-external-folder-menu-item vault-external-folder-menu-item-remove"
                onclick={() => {
                  openRootMenu = null;
                  externalDesk.unpinRoot(root.id);
                }}
              >
                <Unlink size={13} strokeWidth={1.8} />
                <span>Remove from Files</span>
              </button>
            </OverflowMenu>
          </div>

          {#if expanded}
            {#if externalDesk.loadingRoot === root.path}
              <p class="vault-external-group-status">Scanning…</p>
            {:else if total === 0}
              <p class="vault-external-group-status">No files in this folder.</p>
            {:else}
              <div class="vault-external-group-body">
                {#if !showAll(root.id) && total > RECENT_PEEK}
                  <p class="vault-external-recents-label">
                    Recent files <span>· {RECENT_PEEK} of {total}</span>
                  </p>
                {/if}
                <ul class="vault-external-file-list">
                  {#each entries as entry (entry.path)}
                    <li>
                      <ExternalFileRow
                        {entry}
                        selected={externalDesk.selectedExternalPath === entry.path}
                        showLink={canLink}
                        onOpen={handleOpen}
                        onLink={handleLink}
                      />
                    </li>
                  {/each}
                </ul>
                {#if total > RECENT_PEEK && !showAll(root.id)}
                  <button
                    type="button"
                    class="vault-external-group-footer"
                    onclick={() => setShowAll(root.id, true)}
                  >
                    <span>Show {total - RECENT_PEEK} more</span>
                    <ChevronRight size={12} strokeWidth={1.8} />
                  </button>
                {:else if showAll(root.id) && total > RECENT_PEEK}
                  <button
                    type="button"
                    class="vault-external-group-footer"
                    onclick={() => setShowAll(root.id, false)}
                  >
                    <span>Show recent only</span>
                    <ChevronUp size={12} strokeWidth={1.8} />
                  </button>
                {/if}
              </div>
            {/if}
          {/if}
        </section>
      {/each}
    </div>
  {/if}
</div>
