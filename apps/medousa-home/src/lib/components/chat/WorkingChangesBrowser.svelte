<script lang="ts">
  import { ArrowLeft, ArrowUpRight, ChevronRight, LoaderCircle } from "@lucide/svelte";
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { countDiffStats, type DiffFileSection } from "$lib/diff/diffTypes";
  import {
    getChangesFile,
    type ChangesFileDiff,
    type ForgeChanges,
  } from "$lib/forge";
  import { layout } from "$lib/runtime/layout.svelte";

  interface Props {
    workId: string;
    changes: ForgeChanges | null;
    loading?: boolean;
    error?: string | null;
    onOpenFile: (path?: string, line?: number) => void | Promise<void>;
    onRefresh: () => void | Promise<void>;
  }

  let {
    workId,
    changes,
    loading = false,
    error = null,
    onOpenFile,
    onRefresh,
  }: Props = $props();

  let selectedPath = $state<string | null>(null);
  let selectedDiff = $state<ChangesFileDiff | null>(null);
  let fileLoading = $state(false);
  let fileError = $state<string | null>(null);
  let requestSerial = 0;

  function basename(path: string): string {
    return path.replaceAll("\\", "/").split("/").at(-1) || path;
  }

  function parentPath(path: string): string {
    const normalized = path.replaceAll("\\", "/");
    const index = normalized.lastIndexOf("/");
    return index > 0 ? normalized.slice(0, index) : "";
  }

  function statusLabel(status: string): string {
    switch (status) {
      case "added": return "Added";
      case "deleted": return "Deleted";
      case "renamed": return "Renamed";
      case "copied": return "Copied";
      case "type_changed": return "Type changed";
      case "untracked": return "New";
      case "unmerged": return "Conflict";
      default: return "Modified";
    }
  }

  function toStackFile(diff: ChangesFileDiff): DiffFileSection {
    const stats = countDiffStats(diff.hunks);
    return {
      path: diff.path,
      oldPath: diff.old_path,
      status: diff.status,
      binary: diff.binary,
      conflict: diff.conflict,
      additions: stats.additions,
      deletions: stats.deletions,
      hunks: diff.hunks,
      baselineBytes: diff.baseline.byte_size,
      reviewedBytes: diff.working.byte_size,
      baselineExists: diff.baseline.exists,
      reviewedExists: diff.working.exists,
      // The chat review only needs patch hunks. Omitting whole-file content
      // avoids holding three representations of each selected file.
      beforeText: null,
      afterText: null,
    };
  }

  const selectedFiles = $derived(selectedDiff ? [toStackFile(selectedDiff)] : []);

  function clearSelection() {
    requestSerial += 1;
    selectedPath = null;
    selectedDiff = null;
    fileLoading = false;
    fileError = null;
  }

  async function selectFile(path: string) {
    const id = workId.trim();
    if (!id) return;
    const serial = ++requestSerial;
    selectedPath = path;
    selectedDiff = null;
    fileLoading = true;
    fileError = null;
    try {
      const diff = await getChangesFile(id, path, { includeContent: false });
      if (serial !== requestSerial || selectedPath !== path) return;
      selectedDiff = diff;
    } catch (loadError) {
      if (serial !== requestSerial || selectedPath !== path) return;
      fileError = loadError instanceof Error ? loadError.message : String(loadError);
    } finally {
      if (serial === requestSerial) fileLoading = false;
    }
  }

  $effect(() => {
    if (
      selectedPath &&
      changes &&
      !changes.files.some((file) => file.path === selectedPath)
    ) {
      clearSelection();
    }
  });
</script>

{#if selectedPath}
  <div class="working-file-detail">
    <header class="working-file-detail-header">
      <button type="button" class="working-file-back" onclick={clearSelection}>
        <ArrowLeft size={13} />
        Files
      </button>
      <div class="working-file-identity">
        <strong>{basename(selectedPath)}</strong>
        {#if parentPath(selectedPath)}<span>{parentPath(selectedPath)}</span>{/if}
      </div>
      <button
        type="button"
        class="working-file-open"
        onclick={() => void onOpenFile(selectedPath ?? undefined)}
      >
        <ArrowUpRight size={12} />
        Open
      </button>
    </header>

    {#if fileLoading}
      <div class="working-file-empty"><LoaderCircle size={16} class="animate-spin" />Loading diff…</div>
    {:else if fileError}
      <div class="working-file-error">
        <p>{fileError}</p>
        <button
          type="button"
          onclick={() => {
            if (selectedPath) void selectFile(selectedPath);
          }}
        >Try again</button>
      </div>
    {:else if selectedDiff}
      {#if selectedDiff.truncated}
        <p class="working-file-truncated">
          This preview is capped for safety. Open the file in Code to inspect the rest.
        </p>
      {/if}
      <DiffStack
        files={selectedFiles}
        density="compact"
        chrome="prefs"
        wrap={layout.isMobile}
        onOpenFile={(path, line) => onOpenFile(path, line)}
      />
    {/if}
  </div>
{:else if loading && !changes}
  <div class="working-file-empty"><LoaderCircle size={16} class="animate-spin" />Loading changes…</div>
{:else if error}
  <div class="working-file-error">
    <p>{error}</p>
    <button type="button" onclick={() => void onRefresh()}>Try again</button>
  </div>
{:else if changes?.files.length}
  <ul class="working-file-list" aria-label="Changed files">
    {#each changes.files as file (file.path)}
      <li>
        <button type="button" onclick={() => void selectFile(file.path)}>
          <span class="working-file-status" class:working-file-status--conflict={file.status === "unmerged"}>
            {statusLabel(file.status)}
          </span>
          <span class="working-file-list-identity">
            <strong>{basename(file.path)}</strong>
            {#if parentPath(file.path)}<span>{parentPath(file.path)}</span>{/if}
          </span>
          <ChevronRight aria-hidden="true" />
        </button>
      </li>
    {/each}
  </ul>
{:else}
  <div class="working-file-empty">No working changes.</div>
{/if}

<style>
  .working-file-list {
    margin: 0;
    overflow: hidden;
    border: 1px solid rgb(var(--theme-border) / 0.22);
    border-radius: var(--theme-container-radius);
    padding: 0;
    background: rgb(var(--theme-card) / 0.5);
    list-style: none;
  }

  .working-file-list li + li {
    border-top: 1px solid rgb(var(--theme-border) / 0.13);
  }

  .working-file-list button {
    display: grid;
    width: 100%;
    min-width: 0;
    grid-template-columns: 5.25rem minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.7rem;
    padding: 0.58rem 0.7rem;
    text-align: left;
  }

  .working-file-list button:hover,
  .working-file-list button:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.55);
  }

  .working-file-list button > :global(svg) {
    width: 0.8rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .working-file-status {
    overflow: hidden;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.625rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .working-file-status--conflict {
    color: rgb(var(--theme-error));
  }

  .working-file-list-identity,
  .working-file-identity {
    display: flex;
    min-width: 0;
    align-items: baseline;
    gap: 0.45rem;
  }

  .working-file-list-identity strong,
  .working-file-identity strong {
    overflow: hidden;
    color: rgb(var(--theme-text));
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.71875rem;
    font-weight: 550;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .working-file-list-identity span,
  .working-file-identity span {
    min-width: 0;
    overflow: hidden;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.59375rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .working-file-detail {
    min-width: 0;
  }

  .working-file-detail-header {
    display: grid;
    min-width: 0;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.65rem;
    margin-bottom: 0.65rem;
    border-bottom: 1px solid rgb(var(--theme-border) / 0.16);
    padding: 0.1rem 0.05rem 0.55rem;
  }

  .working-file-back,
  .working-file-open,
  .working-file-error button {
    display: inline-flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 0.3rem;
    border-radius: var(--theme-control-radius);
    padding: 0.28rem 0.4rem;
    color: rgb(var(--theme-link));
    font-size: 0.6875rem;
  }

  .working-file-back:hover,
  .working-file-back:focus-visible,
  .working-file-open:hover,
  .working-file-open:focus-visible,
  .working-file-error button:hover,
  .working-file-error button:focus-visible {
    background: rgb(var(--theme-card-hover) / 0.65);
  }

  .working-file-empty,
  .working-file-error {
    display: flex;
    min-height: 12rem;
    align-items: center;
    justify-content: center;
    gap: 0.45rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.75rem;
  }

  .working-file-error {
    flex-direction: column;
    color: rgb(var(--theme-error));
    font-size: 0.6875rem;
    text-align: center;
  }

  .working-file-error p,
  .working-file-truncated {
    margin: 0;
  }

  .working-file-truncated {
    margin-bottom: 0.6rem;
    border: 1px solid rgb(var(--theme-warning) / 0.25);
    border-radius: var(--theme-control-radius);
    padding: 0.45rem 0.55rem;
    background: rgb(var(--theme-warning) / 0.06);
    color: rgb(var(--theme-warning));
    font-size: 0.625rem;
  }

  @media (max-width: 48rem) {
    .working-file-list button {
      grid-template-columns: 4.5rem minmax(0, 1fr) auto;
      min-height: 2.8rem;
    }

    .working-file-list-identity {
      align-items: flex-start;
      flex-direction: column;
      gap: 0.08rem;
    }
  }
</style>
