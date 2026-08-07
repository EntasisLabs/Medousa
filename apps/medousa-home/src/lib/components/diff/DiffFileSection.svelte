<script lang="ts">
  import { Check, Eye, FileCode2, FileQuestion, RotateCcw } from "@lucide/svelte";
  import DiffHunkView from "./DiffHunkView.svelte";
  import { countDiffStats, type DiffFileSection } from "$lib/diff/diffTypes";
  import { languageHintForPath } from "$lib/syntax/highlightDiffLine";

  interface Props {
    file: DiffFileSection;
    mode: "inline" | "side";
    busy?: boolean;
    density?: "comfortable" | "compact";
    collapsed?: boolean;
    viewed?: boolean;
    mountHunks?: boolean;
    onOpenFile?: (path: string, line?: number) => void;
    onRestore?: () => void;
    onRevertHunk?: (hunkIndex: number) => void;
    onToggleCollapsed?: () => void;
    onToggleViewed?: () => void;
    onComment?: (input: {
      path: string;
      side: "new" | "old";
      line: number;
      content: string;
    }) => void;
    /** Footer copy when restore is available. */
    restoreHint?: string;
    restoreLabel?: string;
  }

  let {
    file,
    mode,
    busy = false,
    density = "comfortable",
    collapsed = false,
    viewed = false,
    mountHunks = true,
    onOpenFile,
    onRestore,
    onRevertHunk,
    onToggleCollapsed,
    onToggleViewed,
    onComment,
    restoreHint = "The reviewed revision remains available as a recovery point.",
    restoreLabel = "Restore before this change…",
  }: Props = $props();

  const stats = $derived(
    typeof file.additions === "number" && typeof file.deletions === "number"
      ? { additions: file.additions, deletions: file.deletions }
      : countDiffStats(file.hunks),
  );

  const languageHint = $derived(languageHintForPath(file.path));

  function fileName(path: string): string {
    return path.split("/").at(-1) || path;
  }

  function parentPath(path: string): string {
    const parts = path.split("/");
    parts.pop();
    return parts.join("/");
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${Math.ceil(bytes / 1024)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function binaryMessage(path: string): { title: string; copy: string } {
    if (fileName(path) === ".DS_Store") {
      return {
        title: "macOS folder metadata changed.",
        copy: "This file is created by Finder and usually does not belong in source control.",
      };
    }
    return {
      title: "This file changed, but it has no text preview.",
      copy: "Both exact versions are preserved with the project record.",
    };
  }

  function statusLabel(status?: string): string {
    if (status === "added") return "Added";
    if (status === "deleted") return "Deleted";
    if (status === "renamed") return "Renamed";
    if (status === "copied") return "Copied";
    if (status === "type_changed") return "Type changed";
    if (status === "untracked") return "Untracked";
    if (status === "unmerged") return "Conflict";
    return "Changed";
  }

  const parent = $derived(parentPath(file.path));
  const openLine = $derived(
    file.hunks[0]?.new_start && file.hunks[0].new_start > 0 ? file.hunks[0].new_start : 1,
  );
</script>

<article class="diff-file" class:diff-file--viewed={viewed} class:diff-file--collapsed={collapsed}>
  <header class="diff-file-header">
    <button
      type="button"
      class="diff-file-title"
      onclick={() => onToggleCollapsed?.()}
      aria-expanded={!collapsed}
    >
      <span class="diff-file-icon"><FileCode2 size={14} /></span>
      <div class="diff-file-copy">
        <p>{fileName(file.path)}</p>
        {#if parent}
          <span>{parent}</span>
        {/if}
        <div class="diff-file-meta">
          {#if file.status}
            <span class="diff-file-status">{statusLabel(file.status)}</span>
          {/if}
          {#if !file.binary}
            <span class="diff-file-stats">
              <span class="diff-add">+{stats.additions}</span>
              <span class="diff-del">−{stats.deletions}</span>
            </span>
          {/if}
          {#if file.oldPath}
            <span class="diff-file-moved">Moved from {file.oldPath}</span>
          {/if}
        </div>
      </div>
    </button>
    <div class="diff-file-actions">
      {#if onToggleViewed}
        <button
          type="button"
          class="diff-viewed"
          class:diff-viewed--on={viewed}
          aria-pressed={viewed}
          title={viewed ? "Mark as not viewed" : "Mark as viewed"}
          onclick={() => onToggleViewed()}
        >{#if viewed}<Check size={12} />{:else}<Eye size={12} />{/if}</button>
      {/if}
      {#if onOpenFile && !file.binary}
        <button
          type="button"
          class="diff-open-code"
          onclick={() => onOpenFile?.(file.path, openLine)}
        >Open in Code</button>
      {/if}
    </div>
  </header>

  {#if !collapsed}
    {#if file.binary}
      <div class="diff-binary">
        <span class="diff-binary-icon"><FileQuestion size={22} strokeWidth={1.5} /></span>
        <div>
          <p class="diff-binary-title">{binaryMessage(file.path).title}</p>
          <p class="diff-binary-copy">{binaryMessage(file.path).copy}</p>
        </div>
        {#if file.baselineBytes != null || file.reviewedBytes != null || file.baselineExists != null || file.reviewedExists != null}
          <dl class="diff-binary-facts">
            <div>
              <dt>Before</dt>
              <dd>
                {file.baselineExists === false
                  ? "Not present"
                  : typeof file.baselineBytes === "number"
                    ? formatBytes(file.baselineBytes)
                    : "—"}
              </dd>
            </div>
            <div>
              <dt>After</dt>
              <dd>
                {file.reviewedExists === false
                  ? "Removed"
                  : typeof file.reviewedBytes === "number"
                    ? formatBytes(file.reviewedBytes)
                    : "—"}
              </dd>
            </div>
          </dl>
        {/if}
      </div>
    {:else if file.hunks.length === 0}
      <div class="diff-file-empty">
        <p>No textual differences to show.</p>
      </div>
    {:else if mountHunks}
      <DiffHunkView
        hunks={file.hunks}
        {mode}
        {density}
        languageHint={languageHint}
        beforeText={file.beforeText}
        afterText={file.afterText}
        onRevertHunk={onRevertHunk}
        revertBusy={busy}
        onComment={
          onComment
            ? (input) => onComment({ path: file.path, ...input })
            : undefined
        }
      />
    {:else}
      <div class="diff-file-empty">
        <p>Scroll to load this comparison…</p>
      </div>
    {/if}

    {#if onRestore && !file.binary}
      <footer class="diff-file-footer">
        <p>
          {#if file.conflict}
            Resolve by restoring the project baseline, or edit markers in Code.
          {:else}
            {restoreHint}
          {/if}
        </p>
        <button
          type="button"
          class="diff-restore"
          disabled={busy}
          onclick={() => onRestore?.()}
        ><RotateCcw size={12} />{restoreLabel}</button>
      </footer>
    {/if}
  {/if}
</article>

<style>
  .diff-file {
    border: 1px solid rgb(var(--color-surface-500) / 0.26);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-950) / 0.2);
    overflow: hidden;
  }

  .diff-file--viewed {
    border-color: rgb(var(--color-success-500) / 0.22);
  }

  .diff-file-header {
    display: flex;
    min-height: 2.75rem;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.18);
    padding: 0.5rem 0.75rem;
  }

  .diff-file--collapsed .diff-file-header {
    border-bottom: 0;
  }

  .diff-file-title {
    display: flex;
    min-width: 0;
    align-items: flex-start;
    gap: 0.55rem;
    border: 0;
    background: transparent;
    padding: 0;
    text-align: left;
    color: rgb(var(--theme-text-tertiary));
    cursor: pointer;
  }

  .diff-file-icon {
    margin-top: 0.15rem;
    flex-shrink: 0;
  }

  .diff-file-copy {
    min-width: 0;
  }

  .diff-file-copy > p {
    overflow: hidden;
    font-size: 0.75rem;
    font-weight: 500;
    color: rgb(var(--color-surface-200));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .diff-file-copy > span {
    display: block;
    overflow: hidden;
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-faint));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .diff-file-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem;
    margin-top: 0.15rem;
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-faint));
  }

  .diff-file-stats {
    display: inline-flex;
    gap: 0.35rem;
    font-variant-numeric: tabular-nums;
  }

  .diff-add {
    color: rgb(var(--theme-success));
  }

  .diff-del {
    color: rgb(var(--theme-error));
  }

  .diff-file-actions {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    flex-shrink: 0;
  }

  .diff-open-code,
  .diff-restore,
  .diff-viewed {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.3rem 0.5rem;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.625rem;
  }

  .diff-viewed--on {
    color: rgb(var(--theme-success));
  }

  .diff-open-code:hover,
  .diff-viewed:hover {
    background: rgb(var(--color-primary-500) / 0.1);
    color: rgb(var(--theme-link));
  }

  .diff-binary {
    display: grid;
    gap: 0.75rem;
    padding: 1.25rem 0.9rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .diff-binary-icon {
    color: rgb(var(--theme-text-faint));
  }

  .diff-binary-title {
    font-size: 0.75rem;
    font-weight: 500;
    color: rgb(var(--color-surface-200));
  }

  .diff-binary-copy {
    margin-top: 0.2rem;
    font-size: 0.6875rem;
    line-height: 1.45;
    color: rgb(var(--theme-text-quiet));
  }

  .diff-binary-facts {
    display: flex;
    gap: 1.25rem;
  }

  .diff-binary-facts dt {
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-faint));
  }

  .diff-binary-facts dd {
    margin-top: 0.1rem;
    font-size: 0.6875rem;
    color: rgb(var(--theme-text-secondary));
  }

  .diff-file-empty {
    display: flex;
    min-height: 6rem;
    align-items: center;
    justify-content: center;
    padding: 1.5rem;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.6875rem;
  }

  .diff-file-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.18);
    padding: 0.45rem 0.65rem;
  }

  .diff-file-footer p {
    font-size: 0.5625rem;
    color: rgb(var(--theme-text-faint));
  }

  .diff-restore {
    color: rgb(var(--theme-warning));
  }

  .diff-restore:hover:not(:disabled) {
    background: rgb(var(--color-warning-500) / 0.08);
    color: rgb(var(--theme-warning));
  }

  .diff-restore:disabled {
    opacity: 0.35;
  }
</style>
