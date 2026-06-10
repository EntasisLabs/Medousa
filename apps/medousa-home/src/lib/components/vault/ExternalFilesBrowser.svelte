<script lang="ts">
  import { FolderOpen, Link2, Pin, RefreshCw, Unlink } from "@lucide/svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import {
    formatExternalFileSize,
    formatExternalModified,
  } from "$lib/utils/externalDeskApi";
  import { openAttachmentPath } from "$lib/utils/vaultAttachmentPicker";
  import { canPreviewAttachment } from "$lib/utils/vaultAttachments";
  import type { ExternalFileEntry } from "$lib/types/externalDesk";

  interface Props {
    compact?: boolean;
  }

  let { compact = false }: Props = $props();

  function visibleFiles(rootPath: string): ExternalFileEntry[] {
    return (externalDesk.entriesByRoot[rootPath] ?? []).filter((entry) => !entry.is_dir);
  }

  async function handlePinFolder() {
    await externalDesk.pinFolder();
  }

  async function handleOpen(entry: ExternalFileEntry) {
    externalDesk.selectExternalPath(entry.path);
    await openAttachmentPath(entry.path);
  }

  function handlePreview(entry: ExternalFileEntry) {
    externalDesk.selectExternalPath(entry.path);
    vault.previewAttachment(entry.path);
  }

  function handleLink(entry: ExternalFileEntry) {
    if (!vault.selectedPath) return;
    vault.linkExternalFile(entry.path);
  }
</script>

<div class="flex min-h-0 flex-1 flex-col">
  <div class="flex items-center gap-2 border-b border-surface-500/35 px-3 py-2">
    <button
      type="button"
      class="btn btn-sm variant-soft-primary"
      onclick={() => void handlePinFolder()}
    >
      <Pin size={14} strokeWidth={2} />
      Pin folder
    </button>
    {#if externalDesk.pinnedRoots.length > 0}
      <button
        type="button"
        class="btn btn-sm variant-ghost-surface"
        disabled={Boolean(externalDesk.loadingRoot)}
        onclick={() => void externalDesk.refreshAllRoots()}
      >
        <RefreshCw size={14} strokeWidth={2} />
        Refresh
      </button>
    {/if}
  </div>

  {#if externalDesk.error}
    <p class="border-b border-error-500/30 bg-error-500/10 px-3 py-2 text-xs text-error-300">
      {externalDesk.error}
    </p>
  {/if}

  {#if externalDesk.pinnedRoots.length === 0}
    <div class="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
      <FolderOpen size={28} strokeWidth={1.5} class="text-surface-500" />
      <div class="max-w-xs space-y-1">
        <p class="text-sm font-medium text-surface-100">Your desk lives outside the vault too</p>
        <p class="text-xs text-surface-500">
          Pin Documents, Downloads, or project folders. Link files into notes without importing
          them.
        </p>
      </div>
      <button type="button" class="btn btn-sm variant-filled-primary" onclick={() => void handlePinFolder()}>
        Pin your first folder
      </button>
    </div>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto p-2">
      {#each externalDesk.pinnedRoots as root (root.id)}
        <section class="mb-4">
          <div class="mb-1 flex items-center gap-2 px-1">
            <p class="min-w-0 flex-1 truncate text-xs font-semibold uppercase tracking-wide text-surface-400">
              {root.label}
            </p>
            <button
              type="button"
              class="btn btn-xs variant-ghost-surface"
              aria-label="Unpin folder"
              onclick={() => externalDesk.unpinRoot(root.id)}
            >
              <Unlink size={12} strokeWidth={2} />
            </button>
          </div>
          <p class="mb-2 truncate px-1 text-[10px] text-surface-600" title={root.path}>
            {root.path}
          </p>

          {#if externalDesk.loadingRoot === root.path}
            <p class="px-2 py-3 text-xs text-surface-500">Scanning folder…</p>
          {:else if visibleFiles(root.path).length === 0}
            <p class="px-2 py-2 text-xs text-surface-500">No files found in this folder.</p>
          {:else}
            <ul class="space-y-0.5">
              {#each visibleFiles(root.path).slice(0, compact ? 40 : 120) as entry (entry.path)}
                <li>
                  <div
                    class="group flex items-center gap-2 rounded-container-token px-2 py-1.5 {externalDesk.selectedExternalPath ===
                    entry.path
                      ? 'bg-primary-500/10'
                      : 'hover:bg-surface-700/70'}"
                  >
                    <button
                      type="button"
                      class="min-w-0 flex-1 text-left"
                      onclick={() => void handleOpen(entry)}
                    >
                      <span class="block truncate text-sm text-surface-100">{entry.name}</span>
                      <span class="workshop-faint block truncate text-[10px]">
                        {formatExternalModified(entry.modified_at_utc)}
                        {#if entry.size_bytes > 0}
                          · {formatExternalFileSize(entry.size_bytes)}
                        {/if}
                      </span>
                    </button>
                    <div class="flex shrink-0 gap-1 opacity-0 transition group-hover:opacity-100">
                      {#if canPreviewAttachment(externalDesk.attachmentForPath(entry.path))}
                        <button
                          type="button"
                          class="btn btn-xs variant-ghost-surface"
                          onclick={() => handlePreview(entry)}
                        >
                          Preview
                        </button>
                      {/if}
                      {#if vault.selectedPath}
                        <button
                          type="button"
                          class="btn btn-xs variant-soft-primary"
                          onclick={() => handleLink(entry)}
                          title="Link to open note"
                        >
                          <Link2 size={12} strokeWidth={2} />
                        </button>
                      {/if}
                    </div>
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
        </section>
      {/each}
    </div>
  {/if}
</div>
