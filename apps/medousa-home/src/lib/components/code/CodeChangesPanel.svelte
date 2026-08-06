<script lang="ts">
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { countDiffStats, type DiffFileSection } from "$lib/diff/diffTypes";
  import type { ChangesFileDiff, ForgeChanges } from "$lib/forge";

  type Props = {
    changes: ForgeChanges | null;
    loading?: boolean;
    error?: string | null;
    selectedPath?: string | null;
    fileDiff?: ChangesFileDiff | null;
    fileLoading?: boolean;
    fileError?: string | null;
    restoreBusy?: boolean;
    onSelectPath: (path: string) => void;
    onOpenPath: (path: string, line?: number) => void;
    onRestorePath: (diff: ChangesFileDiff) => void;
    onClose: () => void;
    onRefresh: () => void;
  };

  let {
    changes,
    loading = false,
    error = null,
    selectedPath = null,
    fileDiff = null,
    fileLoading = false,
    fileError = null,
    restoreBusy = false,
    onSelectPath,
    onOpenPath,
    onRestorePath,
    onClose,
    onRefresh,
  }: Props = $props();

  let mode = $state<"inline" | "side">("inline");

  function statusLabel(status: string): string {
    switch (status) {
      case "added":
        return "A";
      case "modified":
        return "M";
      case "deleted":
        return "D";
      case "renamed":
        return "R";
      case "copied":
        return "C";
      case "type_changed":
        return "T";
      case "untracked":
        return "U";
      case "unmerged":
        return "!";
      default:
        return "?";
    }
  }

  function trackingLine(snapshot: ForgeChanges): string {
    const parts: string[] = [];
    if (snapshot.detached) {
      parts.push("detached HEAD");
    } else if (snapshot.branch) {
      parts.push(snapshot.branch);
    }
    if (snapshot.upstream) {
      const ahead = snapshot.ahead ?? 0;
      const behind = snapshot.behind ?? 0;
      if (ahead === 0 && behind === 0) {
        parts.push(`↑ ${snapshot.upstream}`);
      } else {
        parts.push(`↑ ${snapshot.upstream} +${ahead} −${behind}`);
      }
    }
    if (snapshot.base_ref) {
      parts.push(`base ${snapshot.base_ref}`);
    }
    return parts.join(" · ") || "Working copy";
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
      beforeText: diff.baseline.content ?? null,
      afterText: diff.working.content ?? null,
    };
  }

  const stackFiles = $derived(fileDiff ? [toStackFile(fileDiff)] : []);
</script>

<div class="flex max-h-[28rem] shrink-0 flex-col border-t border-surface-500/25 bg-surface-950/90">
  <div class="sticky top-0 z-10 flex items-center justify-between gap-2 bg-surface-950 px-2.5 py-1 text-[9px] uppercase tracking-wider text-content-quiet">
    <span>Changes</span>
    <div class="flex items-center gap-1 normal-case tracking-normal">
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-[9px] text-content-quiet hover:bg-surface-800 hover:text-content-secondary"
        onclick={onRefresh}
      >Refresh</button>
      <button
        type="button"
        class="rounded px-1.5 py-0.5 text-[9px] text-content-quiet hover:bg-surface-800 hover:text-content-secondary"
        onclick={onClose}
      >Hide</button>
    </div>
  </div>
  {#if error}
    <p class="px-2.5 py-2 text-[10px] text-rose-200/90">{error}</p>
  {:else if loading && !changes}
    <p class="px-2.5 py-2 text-[10px] text-content-quiet">Loading changes…</p>
  {:else if changes}
    <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 border-b border-surface-500/15 px-2.5 py-1.5 text-[10px] text-content-secondary">
      <span class="font-mono text-content-tertiary">{trackingLine(changes)}</span>
      {#if changes.conflict}
        <span class="rounded bg-rose-950/50 px-1.5 py-0.5 text-[9px] text-rose-200">Conflict</span>
      {/if}
      <span class="text-content-quiet">{changes.files.length} file{changes.files.length === 1 ? "" : "s"}</span>
    </div>
    <div class="flex min-h-0 flex-1 overflow-hidden">
      <div class="max-h-full w-44 shrink-0 overflow-y-auto border-r border-surface-500/15">
        {#if changes.files.length === 0}
          <p class="px-2.5 py-2 text-[10px] text-content-quiet">Working tree is clean.</p>
        {:else}
          {#each changes.files as file (file.path)}
            <button
              type="button"
              class="flex w-full items-center gap-2 border-b border-surface-500/10 px-2 py-1 text-left text-[10px] hover:bg-surface-800/60 {selectedPath === file.path ? 'bg-surface-800/70 text-content-primary' : 'text-content-secondary'} {file.status === 'unmerged' ? 'text-rose-200/90' : ''}"
              onclick={() => onSelectPath(file.path)}
            >
              <span class="w-3 shrink-0 font-mono text-[9px] text-content-quiet" title={file.status}>{statusLabel(file.status)}</span>
              <span class="min-w-0 flex-1 truncate font-mono">{file.path}</span>
            </button>
          {/each}
        {/if}
      </div>
      <div class="min-h-0 min-w-0 flex-1 overflow-y-auto px-2 py-1.5">
        {#if !selectedPath}
          <p class="px-1 py-2 text-[10px] text-content-quiet">Select a file to compare against the project baseline.</p>
        {:else if fileLoading && !fileDiff}
          <p class="px-1 py-2 text-[10px] text-content-quiet">Loading diff…</p>
        {:else if fileError}
          <p class="px-1 py-2 text-[10px] text-rose-200/90">{fileError}</p>
        {:else if fileDiff}
          {#if fileDiff.conflict}
            <div class="mb-2 rounded border border-rose-500/30 bg-rose-950/30 px-2 py-1.5 text-[10px] text-rose-100/90">
              Merge conflict on <span class="font-mono">{fileDiff.path}</span>. Open in Code to edit markers, or restore the baseline.
            </div>
          {/if}
          {#if fileDiff.truncated}
            <p class="mb-1 text-[9px] text-amber-200/80">Diff preview was truncated for size.</p>
          {/if}
          <DiffStack
            files={stackFiles}
            bind:mode
            showJumpList={false}
            busy={restoreBusy}
            onOpenFile={(path, line) => onOpenPath(path, line)}
            onRestoreFile={() => onRestorePath(fileDiff)}
            restoreHint="Restore this path to the project baseline in the working copy."
            restoreLabel="Restore baseline…"
          />
        {/if}
      </div>
    </div>
  {:else}
    <p class="px-2.5 py-2 text-[10px] text-content-quiet">Open Changes to load branch and file status.</p>
  {/if}
</div>
