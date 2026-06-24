<script lang="ts">
  import { FolderOpen, RefreshCw, Unlink } from "@lucide/svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { canPreviewAttachment } from "$lib/utils/vaultAttachments";
  import type { ExternalFileEntry, PinnedRoot } from "$lib/types/externalDesk";
  import ExternalFileRow from "./ExternalFileRow.svelte";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  const RECENT_PEEK = compact ? 6 : 10;

  let expandedPins = $state<Record<string, boolean>>({});
  let showAllByRoot = $state<Record<string, boolean>>({});

  const canLink = $derived(Boolean(vault.selectedPath));

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
    return expandedPins[rootId] ?? false;
  }

  function togglePin(rootId: string) {
    expandedPins = { ...expandedPins, [rootId]: !isExpanded(rootId) };
  }

  function showAll(rootId: string): boolean {
    return showAllByRoot[rootId] ?? false;
  }

  function setShowAll(rootId: string, value: boolean) {
    showAllByRoot = { ...showAllByRoot, [rootId]: value };
  }

  function filesToShow(root: PinnedRoot): { entries: ExternalFileEntry[]; total: number } {
    const all = sortedFiles(root.path);
    if (showAll(root.id)) {
      return { entries: all, total: all.length };
    }
    return { entries: all.slice(0, RECENT_PEEK), total: all.length };
  }

  async function handleOpen(entry: ExternalFileEntry) {
    externalDesk.selectExternalPath(entry.path);
    const attachment = externalDesk.attachmentForPath(entry.path);
    if (canPreviewAttachment(attachment)) {
      vault.previewAttachment(entry.path);
      return;
    }
    await openAttachmentPath(entry.path);
  }

  function handleLink(entry: ExternalFileEntry) {
    if (!vault.selectedPath) return;
    vault.linkExternalFile(entry.path);
  }

  async function handleRefreshRoot(root: PinnedRoot, event: MouseEvent) {
    event.stopPropagation();
    await externalDesk.refreshRoot(root.path);
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  {#if externalDesk.error}
    <p class="border-b border-error-500/30 bg-error-500/10 px-3 py-2 text-xs text-error-300">
      {externalDesk.error}
    </p>
  {/if}

  {#if externalDesk.pinnedRoots.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
      <FolderOpen size={28} strokeWidth={1.5} class="text-surface-500" />
      <div class="max-w-xs space-y-1">
        <p class="text-sm font-medium text-surface-100">Pin folders from your Mac</p>
        <p class="text-xs leading-relaxed text-surface-500">
          Use <span class="text-surface-400">Pin folder</span> above, then search or expand a pin
          to link files into your notes.
        </p>
      </div>
    </div>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto p-2">
      <p class="mb-2 px-1 text-[10px] leading-relaxed text-surface-500">
        {#if canLink}
          Click a file to open · hover to link to your note
        {:else}
          Open a note to link files · search pinned folders above
        {/if}
      </p>

      {#each externalDesk.pinnedRoots as root (root.id)}
        {@const expanded = isExpanded(root.id)}
        {@const { entries, total } = filesToShow(root)}
        <section class="mb-1">
          <div class="flex items-center gap-0.5">
            <button
              type="button"
              class="vault-external-pin-row min-w-0 flex-1"
              aria-expanded={expanded}
              title={root.path}
              onclick={() => togglePin(root.id)}
            >
              <span class="workshop-faint w-4 shrink-0 text-center text-[10px]">
                {expanded ? "▾" : "▸"}
              </span>
              <FolderOpen size={14} strokeWidth={1.75} class="shrink-0 text-surface-400" />
              <span class="min-w-0 flex-1 truncate text-sm font-medium text-surface-100">
                {root.label}
              </span>
              {#if total > 0}
                <span class="shrink-0 tabular-nums text-[10px] text-surface-500">{total}</span>
              {/if}
            </button>
            <button
              type="button"
              class="vault-toolbar-btn"
              aria-label="Refresh folder"
              disabled={externalDesk.loadingRoot === root.path}
              onclick={(event) => void handleRefreshRoot(root, event)}
            >
              <RefreshCw size={12} strokeWidth={2} />
            </button>
            <button
              type="button"
              class="vault-toolbar-btn"
              aria-label="Unpin folder"
              onclick={(event) => {
                event.stopPropagation();
                externalDesk.unpinRoot(root.id);
              }}
            >
              <Unlink size={12} strokeWidth={2} />
            </button>
          </div>

          {#if expanded}
            {#if externalDesk.loadingRoot === root.path}
              <p class="px-6 py-2 text-xs text-surface-500">Scanning…</p>
            {:else if total === 0}
              <p class="px-6 py-2 text-xs text-surface-500">No files in this folder.</p>
            {:else}
              <ul class="mt-0.5 space-y-0.5 pl-3">
                {#if !showAll(root.id) && total > RECENT_PEEK}
                  <li class="px-2 py-1 text-[10px] text-surface-500">
                    Recent · {RECENT_PEEK} of {total}
                  </li>
                {/if}
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
                {#if total > RECENT_PEEK && !showAll(root.id)}
                  <li class="px-1 pt-0.5">
                    <button
                      type="button"
                      class="w-full rounded-md px-2 py-1.5 text-left text-xs text-primary-300 hover:bg-surface-800/60 hover:text-primary-200"
                      onclick={() => setShowAll(root.id, true)}
                    >
                      Show all {total} files…
                    </button>
                  </li>
                {:else if showAll(root.id) && total > RECENT_PEEK}
                  <li class="px-1 pt-0.5">
                    <button
                      type="button"
                      class="w-full rounded-md px-2 py-1.5 text-left text-xs text-surface-500 hover:bg-surface-800/60 hover:text-surface-300"
                      onclick={() => setShowAll(root.id, false)}
                    >
                      Show recent only
                    </button>
                  </li>
                {/if}
              </ul>
            {/if}
          {/if}
        </section>
      {/each}
    </div>
  {/if}
</div>
