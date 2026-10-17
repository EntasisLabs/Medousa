<script lang="ts">
  import "$lib/styles/files-surface.postcss";
  import {
    Check,
    ChevronDown,
    ChevronRight,
    Ellipsis,
    Eye,
    Folder,
    FolderPlus,
    FolderOpen,
    RefreshCw,
    SlidersHorizontal,
    Unlink,
  } from "@lucide/svelte";
  import OverflowMenu from "$lib/components/ui/OverflowMenu.svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { canPreviewAttachment } from "$lib/utils/vaultAttachments";
  import type { ExternalFileEntry, PinnedRoot } from "$lib/types/externalDesk";
  import ExternalDirectoryContents from "./ExternalDirectoryContents.svelte";
  import { isCoLocatedWorkshop, vaultPinFolderRemoteHint } from "$lib/utils/workshopLocality";
  import { hostComputerPhrase, onThisHostPhrase } from "$lib/platformCopy";

  interface Props {
    /** When set (LME), open goes through workspace tabs instead of pane preview only. */
    onOpenFile?: (entry: ExternalFileEntry) => void | Promise<void>;
  }

  let { onOpenFile }: Props = $props();

  const canLink = $derived(Boolean(vault.selectedPath));
  const coLocated = $derived(isCoLocatedWorkshop());
  let openRootMenu = $state<string | null>(null);
  let viewMenuOpen = $state(false);

  function isExpanded(rootId: string): boolean {
    return externalDesk.isPinExpanded(rootId);
  }

  function togglePin(rootId: string) {
    externalDesk.togglePinExpanded(rootId);
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

<div class="files-browser flex min-h-0 flex-1 flex-col">
  {#if externalDesk.error}
    <p class="border-b border-error-500/30 bg-error-500/10 px-3 py-2 text-xs text-content-error">
      {externalDesk.error}
    </p>
  {/if}

  {#if !coLocated}
    <div class="files-empty">
      <span class="files-empty-icon"><Folder size={23} strokeWidth={1.5} /></span>
      <h3>Your files stay {onThisHostPhrase()}</h3>
        <p>
          {vaultPinFolderRemoteHint()}
        </p>
    </div>
  {:else if externalDesk.pinnedRoots.length === 0}
    <div class="files-empty">
      <span class="files-empty-icon"><FolderOpen size={24} strokeWidth={1.4} /></span>
      <h3>Start with a folder</h3>
      <p>Browse documents, images, and code from {hostComputerPhrase()}.</p>
      <button type="button" class="files-button files-button--primary" onclick={() => void externalDesk.pinFolder()}><FolderPlus size={13} />Add folder</button>
    </div>
  {:else}
    <div class="files-section-bar">
      <span class="files-section-label">Folders <span class="files-count">{externalDesk.pinnedRoots.length}</span></span>
      <OverflowMenu open={viewMenuOpen} onOpenChange={(open) => viewMenuOpen = open} panelClass="vault-external-folder-menu" panelWidth={190}>
        {#snippet trigger({ open, toggle })}
          <button type="button" class="files-icon-button" aria-label="File view options" title="View options" aria-haspopup="menu" aria-expanded={open} onclick={toggle}><SlidersHorizontal size={13} /></button>
        {/snippet}
        <button type="button" role="menuitemcheckbox" aria-checked={externalDesk.showHidden} class="vault-external-folder-menu-item" onclick={() => { externalDesk.setShowHidden(!externalDesk.showHidden); viewMenuOpen = false; }}>
          <Eye size={13} /><span class="flex-1">Show hidden files</span>{#if externalDesk.showHidden}<Check size={12} />{/if}
        </button>
      </OverflowMenu>
    </div>
    <div class="vault-external-groups min-h-0 flex-1 overflow-y-auto">
      {#each externalDesk.pinnedRoots as root (root.id)}
        {@const expanded = isExpanded(root.id)}
        {@const directory = externalDesk.directories[root.path]}
        {@const parent = root.path.replace(/\\/g, '/').split('/').filter(Boolean).at(-2)}
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
              <span class="files-root-icon">{#if expanded}<FolderOpen size={15} strokeWidth={1.6} />{:else}<Folder size={15} strokeWidth={1.6} />{/if}</span>
              <span class="files-root-copy"><span class="files-root-name">{root.label}</span>{#if parent}<span class="files-root-location">{parent}</span>{/if}</span>
              {#if directory && !directory.error}<span class="files-count" title="Items on this page">{directory.entries.length}{directory.next_cursor ? '+' : ''}</span>{/if}
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
                disabled={externalDesk.directories[root.path]?.loading ?? false}
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
            <div class="vault-external-group-body">
              <ExternalDirectoryContents path={root.path} showLink={canLink} onOpen={handleOpen} onLink={handleLink} />
            </div>
          {/if}
        </section>
      {/each}
    </div>
  {/if}
</div>
