<script lang="ts">
  import { untrack } from "svelte";
  import { AlertCircle, ChevronDown, ChevronLeft, ChevronRight, Folder, FolderOpen, LoaderCircle, RefreshCw } from "@lucide/svelte";
  import { externalDesk } from "$lib/stores/externalDesk.svelte";
  import type { ExternalFileEntry } from "$lib/types/externalDesk";
  import ExternalFileRow from "./ExternalFileRow.svelte";
  import ExternalDirectoryContents from "./ExternalDirectoryContents.svelte";

  let { path, showLink = false, onOpen, onLink }: {
    path: string;
    showLink?: boolean;
    onOpen: (entry: ExternalFileEntry) => void;
    onLink?: (entry: ExternalFileEntry) => void;
  } = $props();
  const directory = $derived(externalDesk.directories[path]);
  $effect(() => {
    if (!directory) {
      const current = path;
      untrack(() => { void externalDesk.loadDirectory(current); });
    }
  });
</script>

<div class="min-w-0">
  {#if directory?.error}
    <div class="files-tree-feedback files-tree-feedback--error" role="alert">
      <AlertCircle size={13} />
      <div><p>{directory.error}</p><button type="button" class="files-button files-button--quiet" onclick={() => void externalDesk.loadDirectory(path, true)}><RefreshCw size={11} />Retry folder</button></div>
    </div>
  {/if}
  {#if directory?.loading}
    <div class="files-tree-feedback" role="status"><LoaderCircle size={13} class="animate-spin" /><p>{directory.entries.length ? 'Updating folder…' : 'Opening folder…'}</p></div>
  {:else if directory && !directory.error && directory.entries.length === 0}
    <div class="files-tree-feedback"><FolderOpen size={13} /><div><p>{externalDesk.showHidden ? 'This folder is empty.' : 'No visible items.'}</p>{#if !externalDesk.showHidden}<p class="files-tree-hint">Hidden items are off.</p>{/if}</div></div>
  {/if}
  <ul class="files-tree-list" aria-label="Folder contents">
    {#each directory?.entries ?? [] as entry (entry.path)}
      <li class="min-w-0">
        {#if entry.is_dir}
          {@const expanded = externalDesk.expandedDirectories[entry.path] ?? false}
          <div class="files-folder-shell">
            <button type="button" class="files-folder-row" title={entry.path} aria-expanded={expanded} onclick={() => externalDesk.toggleDirectory(entry.path)}>
              {#if expanded}<ChevronDown size={12} /><FolderOpen size={14} />{:else}<ChevronRight size={12} /><Folder size={14} />{/if}<span class="truncate">{entry.name}</span>
            </button>
            {#if expanded}
              <button type="button" class="files-icon-button files-folder-refresh" aria-label="Refresh {entry.name}" disabled={externalDesk.directories[entry.path]?.loading} onclick={() => void externalDesk.refreshRoot(entry.path)}><RefreshCw size={12} /></button>
            {/if}
          </div>
          {#if expanded}
            <div class="files-tree-branch"><ExternalDirectoryContents path={entry.path} {showLink} {onOpen} {onLink} /></div>
          {/if}
        {:else}
          <ExternalFileRow {entry} compact={true} selected={externalDesk.selectedExternalPath === entry.path} {showLink} {onOpen} {onLink} />
        {/if}
      </li>
    {/each}
  </ul>
  {#if directory && (directory.previous.length > 0 || directory.next_cursor)}
    <div class="files-pager">
      <button type="button" class="files-button files-button--quiet" disabled={directory.loading || directory.previous.length === 0} onclick={() => void externalDesk.changeDirectoryPage(path, 'previous')}><ChevronLeft size={11} />Previous</button>
      <span>Page {directory.previous.length + 1}</span>
      <button type="button" class="files-button files-button--quiet" disabled={directory.loading || !directory.next_cursor} onclick={() => void externalDesk.changeDirectoryPage(path, 'next')}>Next<ChevronRight size={11} /></button>
    </div>
  {/if}
</div>
