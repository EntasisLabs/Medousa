<script lang="ts">
  import DiffStack from "$lib/components/diff/DiffStack.svelte";
  import { countDiffStats, type DiffFileSection } from "$lib/diff/diffTypes";
  import type {
    ChangesBlameHunk,
    ChangesFileDiff,
    ChangesHistoryEntry,
    ForgeChanges,
  } from "$lib/forge";

  type Props = {
    changes: ForgeChanges | null;
    attachedCheckout?: boolean;
    loading?: boolean;
    error?: string | null;
    selectedPath?: string | null;
    fileDiff?: ChangesFileDiff | null;
    fileLoading?: boolean;
    fileError?: string | null;
    restoreBusy?: boolean;
    syncBusy?: boolean;
    syncMessage?: string | null;
    history?: ChangesHistoryEntry[];
    historyOpen?: boolean;
    blameHunks?: ChangesBlameHunk[] | null;
    blameOpen?: boolean;
    onSelectPath: (path: string) => void;
    onOpenPath: (path: string, line?: number) => void;
    onRestorePath: (diff: ChangesFileDiff) => void;
    onRevertHunk: (diff: ChangesFileDiff, hunkIndex: number) => void;
    onResolveConflict: (
      diff: ChangesFileDiff,
      resolution: "ours" | "theirs" | "baseline",
    ) => void;
    onFetch: () => void;
    onPull: () => void;
    onPush: () => void;
    onSync: () => void;
    onCheckpoint: () => void;
    onToggleHistory: () => void;
    onToggleBlame: () => void;
    onClose: () => void;
    onRefresh: () => void;
  };

  let {
    changes,
    attachedCheckout = false,
    loading = false,
    error = null,
    selectedPath = null,
    fileDiff = null,
    fileLoading = false,
    fileError = null,
    restoreBusy = false,
    syncBusy = false,
    syncMessage = null,
    history = [],
    historyOpen = false,
    blameHunks = null,
    blameOpen = false,
    onSelectPath,
    onOpenPath,
    onRestorePath,
    onRevertHunk,
    onResolveConflict,
    onFetch,
    onPull,
    onPush,
    onSync,
    onCheckpoint,
    onToggleHistory,
    onToggleBlame,
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
  const historyMutationBlocked = $derived(attachedCheckout);
  const pullBlocked = $derived(
    historyMutationBlocked || !!changes?.conflict || !!changes?.merge_in_progress || (changes?.dirty && (changes?.behind ?? 0) > 0),
  );
  const syncBlocked = $derived(historyMutationBlocked || !!changes?.conflict || !!changes?.merge_in_progress);
</script>

<div class="flex max-h-[32rem] shrink-0 flex-col border-t border-surface-500/25 bg-surface-950/90">
  <div class="sticky top-0 z-10 flex flex-wrap items-center justify-between gap-2 bg-surface-950 px-2.5 py-1 text-chrome-xs uppercase tracking-wider text-content-quiet">
    <span>Changes</span>
    <div class="flex flex-wrap items-center gap-1 normal-case tracking-normal">
      <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800 hover:text-content-secondary disabled:opacity-40" disabled={syncBusy} onclick={onFetch}>Fetch</button>
      <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800 hover:text-content-secondary disabled:opacity-40" disabled={syncBusy || pullBlocked} onclick={onPull} title={historyMutationBlocked ? "Close this project before changing current-checkout Git history" : pullBlocked ? "Resolve conflicts or seal local edits first" : "Fast-forward pull"}>Pull</button>
      <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800 hover:text-content-secondary disabled:opacity-40" disabled={syncBusy || syncBlocked} onclick={onPush} title={historyMutationBlocked ? "Close this project before pushing the current checkout" : "Push branch"}>Push</button>
      <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-link hover:bg-surface-800 disabled:opacity-40" disabled={syncBusy || syncBlocked} onclick={onSync} title={historyMutationBlocked ? "Close this project before syncing the current checkout" : "Sync branch"}>Sync</button>
      <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-amber-200/90 hover:bg-surface-800 disabled:opacity-40" disabled={syncBusy} onclick={onCheckpoint}>Seal for Review</button>
      <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800 hover:text-content-secondary {historyOpen ? 'bg-surface-800 text-content-secondary' : ''}" onclick={onToggleHistory}>History</button>
      <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800 hover:text-content-secondary" onclick={onRefresh}>Refresh</button>
      <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800 hover:text-content-secondary" onclick={onClose}>Hide</button>
    </div>
  </div>
  {#if syncMessage}
    <p class="border-b border-surface-500/15 px-2.5 py-1 text-chrome-sm text-content-tertiary">{syncMessage}</p>
  {/if}
  {#if error}
    <p class="px-2.5 py-2 text-chrome-sm text-rose-200/90">{error}</p>
  {:else if loading && !changes}
    <p class="px-2.5 py-2 text-chrome-sm text-content-quiet">Loading changes…</p>
  {:else if changes}
    <div class="flex flex-wrap items-center gap-x-2 gap-y-0.5 border-b border-surface-500/15 px-2.5 py-1.5 text-chrome-sm text-content-secondary">
      <span class="font-mono text-content-tertiary">{trackingLine(changes)}</span>
      {#if changes.conflict}
        <span class="rounded bg-rose-950/50 px-1.5 py-0.5 text-chrome-xs text-rose-200">Conflict</span>
      {/if}
      {#if changes.merge_in_progress}
        <span class="rounded bg-amber-950/40 px-1.5 py-0.5 text-chrome-xs text-amber-100">Merge in progress</span>
      {/if}
      <span class="text-content-quiet">{changes.files.length} file{changes.files.length === 1 ? "" : "s"}</span>
    </div>
    {#if historyOpen}
      <div class="max-h-28 shrink-0 overflow-y-auto border-b border-surface-500/15">
        {#if history.length === 0}
          <p class="px-2.5 py-2 text-chrome-sm text-content-quiet">{attachedCheckout ? "No repository history." : "No commits since the project baseline."}</p>
        {:else}
          {#each history as commit (commit.oid)}
            <div class="border-b border-surface-500/10 px-2.5 py-1 text-chrome-sm text-content-secondary">
              <span class="font-mono text-content-quiet">{commit.oid.slice(0, 8)}</span>
              <span class="ml-2">{commit.subject}</span>
              <span class="ml-2 text-content-faint">{commit.author_name}</span>
            </div>
          {/each}
        {/if}
      </div>
    {/if}
    <div class="flex min-h-0 flex-1 overflow-hidden">
      <div class="max-h-full w-44 shrink-0 overflow-y-auto border-r border-surface-500/15">
        {#if changes.files.length === 0}
          <p class="px-2.5 py-2 text-chrome-sm text-content-quiet">{attachedCheckout ? "No changes since Coder attached." : "Working tree is clean."}</p>
        {:else}
          {#each changes.files as file (file.path)}
            <button
              type="button"
              class="flex w-full items-center gap-2 border-b border-surface-500/10 px-2 py-1 text-left text-chrome-sm hover:bg-surface-800/60 {selectedPath === file.path ? 'bg-surface-800/70 text-content-primary' : 'text-content-secondary'} {file.status === 'unmerged' ? 'text-rose-200/90' : ''}"
              onclick={() => onSelectPath(file.path)}
            >
              <span class="w-3 shrink-0 font-mono text-chrome-xs text-content-quiet" title={file.status}>{statusLabel(file.status)}</span>
              <span class="min-w-0 flex-1 truncate font-mono">{file.path}</span>
            </button>
          {/each}
        {/if}
      </div>
      <div class="min-h-0 min-w-0 flex-1 overflow-y-auto px-2 py-1.5">
        {#if !selectedPath}
          <p class="px-1 py-2 text-chrome-sm text-content-quiet">Select a file to compare against the project baseline.</p>
        {:else if fileLoading && !fileDiff}
          <p class="px-1 py-2 text-chrome-sm text-content-quiet">Loading diff…</p>
        {:else if fileError}
          <p class="px-1 py-2 text-chrome-sm text-rose-200/90">{fileError}</p>
        {:else if fileDiff}
          {#if fileDiff.conflict}
            <div class="mb-2 rounded border border-rose-500/30 bg-rose-950/30 px-2 py-1.5 text-chrome-sm text-rose-100/90">
              <p class="mb-1">Merge conflict on <span class="font-mono">{fileDiff.path}</span>.</p>
              <div class="flex flex-wrap gap-1">
                <button type="button" class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs hover:bg-surface-700 disabled:opacity-40" disabled={restoreBusy} onclick={() => onResolveConflict(fileDiff, "ours")}>Keep ours</button>
                <button type="button" class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs hover:bg-surface-700 disabled:opacity-40" disabled={restoreBusy} onclick={() => onResolveConflict(fileDiff, "theirs")}>Take theirs</button>
                <button type="button" class="rounded bg-surface-800 px-1.5 py-0.5 text-chrome-xs hover:bg-surface-700 disabled:opacity-40" disabled={restoreBusy} onclick={() => onResolveConflict(fileDiff, "baseline")}>Use baseline</button>
                <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-link hover:bg-surface-800" onclick={() => onOpenPath(fileDiff.path, 1)}>Edit in Code</button>
              </div>
            </div>
          {/if}
          <div class="mb-1 flex items-center gap-1">
            <button type="button" class="rounded px-1.5 py-0.5 text-chrome-xs text-content-quiet hover:bg-surface-800 {blameOpen ? 'bg-surface-800 text-content-secondary' : ''}" onclick={onToggleBlame}>Blame</button>
          </div>
          {#if blameOpen}
            <div class="mb-2 max-h-24 overflow-y-auto rounded border border-surface-500/20">
              {#if !blameHunks}
                <p class="px-2 py-1 text-chrome-xs text-content-quiet">Loading blame…</p>
              {:else if blameHunks.length === 0}
                <p class="px-2 py-1 text-chrome-xs text-content-quiet">No blame available.</p>
              {:else}
                {#each blameHunks as hunk (`${hunk.oid}:${hunk.start_line}`)}
                  <button type="button" class="flex w-full items-center gap-2 border-b border-surface-500/10 px-2 py-0.5 text-left text-chrome-xs text-content-secondary hover:bg-surface-800/60" onclick={() => onOpenPath(fileDiff.path, hunk.start_line)}>
                    <span class="font-mono text-content-quiet">{hunk.oid.slice(0, 7)}</span>
                    <span class="min-w-0 flex-1 truncate">{hunk.summary || hunk.author_name}</span>
                    <span class="text-content-faint">L{hunk.start_line}{hunk.line_count > 1 ? `–${hunk.start_line + hunk.line_count - 1}` : ""}</span>
                  </button>
                {/each}
              {/if}
            </div>
          {/if}
          {#if fileDiff.truncated}
            <p class="mb-1 text-chrome-xs text-amber-200/80">Diff preview was truncated for size.</p>
          {/if}
          <DiffStack
            files={stackFiles}
            bind:mode
            showJumpList={false}
            busy={restoreBusy}
            onOpenFile={(path, line) => onOpenPath(path, line)}
            onRestoreFile={() => onRestorePath(fileDiff)}
            onRevertHunk={(_path, hunkIndex) => onRevertHunk(fileDiff, hunkIndex)}
            restoreHint="Restore this path to the project baseline in the working copy."
            restoreLabel="Restore baseline…"
          />
        {/if}
      </div>
    </div>
  {:else}
    <p class="px-2.5 py-2 text-chrome-sm text-content-quiet">Open Changes to load branch and file status.</p>
  {/if}
</div>
