<script lang="ts">
  import {
    Columns2,
    FileCode2,
    FileQuestion,
    LoaderCircle,
    RotateCcw,
    Rows3,
  } from "@lucide/svelte";
  import { getReviewFile, type ReviewFileDiff, type ReviewProjection } from "$lib/forge";

  interface Props {
    review: ReviewProjection;
    busy?: boolean;
    onOpenFile: (path: string, line?: number) => void | Promise<void>;
    onRestore: (comparison: ReviewFileDiff) => Promise<void>;
  }

  let { review, busy = false, onOpenFile, onRestore }: Props = $props();
  let selectedPath = $state("");
  let comparison = $state<ReviewFileDiff | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let mode = $state<"inline" | "side">("inline");

  const selectedFile = $derived(
    review.changed_files.find((file) => file.path === selectedPath) ?? null,
  );
  const multipleFiles = $derived(review.changed_files.length > 1);
  const selectedIsBinary = $derived(comparison?.binary ?? selectedFile?.is_binary ?? false);

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

  function statusLabel(status: string): string {
    if (status === "added") return "Added";
    if (status === "deleted") return "Deleted";
    if (status === "renamed") return "Renamed";
    if (status === "copied") return "Copied";
    if (status === "type_changed") return "Type changed";
    return "Changed";
  }

  function select(path: string) {
    selectedPath = path;
  }

  function sideRows(diff: ReviewFileDiff) {
    return diff.hunks.flatMap((hunk) => {
      const rows: Array<{
        key: string;
        oldNumber?: number | null;
        newNumber?: number | null;
        oldContent: string;
        newContent: string;
        kind: string;
      }> = [];
      for (let index = 0; index < hunk.lines.length; ) {
        const line = hunk.lines[index];
        if (line.kind === "context") {
          rows.push({
            key: `${hunk.old_start}:${hunk.new_start}:context:${index}`,
            oldNumber: line.old_line,
            newNumber: line.new_line,
            oldContent: line.content,
            newContent: line.content,
            kind: "context",
          });
          index += 1;
          continue;
        }
        const block = [];
        while (index < hunk.lines.length && hunk.lines[index].kind !== "context") {
          block.push(hunk.lines[index]);
          index += 1;
        }
        const deletions = block.filter((entry) => entry.kind === "deletion");
        const additions = block.filter((entry) => entry.kind === "addition");
        for (let offset = 0; offset < Math.max(deletions.length, additions.length); offset += 1) {
          const oldLine = deletions[offset];
          const newLine = additions[offset];
          rows.push({
            key: `${hunk.old_start}:${hunk.new_start}:change:${index}:${offset}`,
            oldNumber: oldLine?.old_line,
            newNumber: newLine?.new_line,
            oldContent: oldLine?.content ?? "",
            newContent: newLine?.content ?? "",
            kind: oldLine && newLine ? "replacement" : oldLine ? "deletion" : "addition",
          });
        }
      }
      return rows;
    });
  }

  async function restore() {
    if (!comparison || busy) return;
    const label = comparison.baseline.exists ? "restore its starting version" : "remove the added file";
    if (!window.confirm(`Reopen this project and ${label}? The reviewed revision stays saved as a recovery point.`)) return;
    await onRestore(comparison);
  }

  $effect(() => {
    const evidence = review.evidence_id;
    const paths = review.changed_files.map((file) => file.path);
    if (!selectedPath || !paths.includes(selectedPath)) selectedPath = paths[0] ?? "";
    if (!evidence || !selectedPath) {
      comparison = null;
      return;
    }
    const path = selectedPath;
    let cancelled = false;
    loading = true;
    error = null;
    void getReviewFile(review.work_id, path)
      .then((result) => {
        if (!cancelled) comparison = result;
      })
      .catch((err) => {
        if (!cancelled) {
          comparison = null;
          error = err instanceof Error ? err.message : String(err);
        }
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });
</script>

<section class="review-surface" aria-label="Change review">
  <div class="review-summary">
    <div class="review-summary-copy">
      <p class="review-summary-kicker">Ready for review</p>
      <p class="review-summary-text">{review.synthesis.status_summary}</p>
    </div>
    <div class="review-signals" aria-label="Review signals">
      <span
        class="review-signal review-signal--{review.synthesis.risk}"
        title={review.synthesis.risk_summary}
      >{review.synthesis.risk} risk</span>
      <span
        class="review-signal {review.synthesis.verification?.success
          ? 'review-signal--passed'
          : 'review-signal--unchecked'}"
        title={review.synthesis.verification
          ? `${review.synthesis.verification.label} · ${review.synthesis.verification.command.join(" ")}`
          : "No project check was recorded"}
      >{review.synthesis.verification?.success ? "Checked" : "Not checked"}</span>
    </div>
  </div>

  {#if review.changed_files.length === 0}
    <div class="review-empty">
      <p>There are no file changes in this revision.</p>
    </div>
  {:else}
    <div
      class="review-change-shell"
      class:review-change-shell--single={!multipleFiles}
      class:review-change-shell--compact={selectedIsBinary}
    >
      {#if multipleFiles}
        <nav class="review-file-rail" aria-label="Changed files">
          <p class="review-file-rail-label">Files</p>
          <div class="review-file-list">
            {#each review.changed_files as file (file.path)}
              <button
                type="button"
                class="review-file-row"
                class:review-file-row--active={selectedPath === file.path}
                onclick={() => select(file.path)}
              >
                {#if file.is_binary}
                  <FileQuestion size={13} strokeWidth={1.7} aria-hidden="true" />
                {:else}
                  <FileCode2 size={13} strokeWidth={1.7} aria-hidden="true" />
                {/if}
                <span class="review-file-row-copy">
                  <span class="review-file-row-name">{fileName(file.path)}</span>
                  <span class="review-file-row-path">{parentPath(file.path)}</span>
                </span>
                <span class="review-file-row-state">{statusLabel(file.status)}</span>
              </button>
            {/each}
          </div>
        </nav>
      {/if}

      <div class="review-file-stage">
        <header class="review-file-header">
          <div class="review-file-title">
            {#if selectedIsBinary}
              <FileQuestion size={15} strokeWidth={1.7} aria-hidden="true" />
            {:else}
              <FileCode2 size={15} strokeWidth={1.7} aria-hidden="true" />
            {/if}
            <div class="min-w-0">
              <p>{fileName(selectedPath) || "Changed file"}</p>
              {#if parentPath(selectedPath)}
                <span>{parentPath(selectedPath)}</span>
              {/if}
              {#if selectedFile?.old_path}
                <span>Moved from {selectedFile.old_path}</span>
              {/if}
            </div>
          </div>
          {#if comparison && !comparison.binary}
            <div class="review-file-actions">
              <div class="review-mode" aria-label="Diff layout">
                <button
                  type="button"
                  class:review-mode-active={mode === "inline"}
                  aria-label="Inline comparison"
                  title="Inline comparison"
                  onclick={() => (mode = "inline")}
                ><Rows3 size={13} /></button>
                <button
                  type="button"
                  class:review-mode-active={mode === "side"}
                  aria-label="Side-by-side comparison"
                  title="Side-by-side comparison"
                  onclick={() => (mode = "side")}
                ><Columns2 size={13} /></button>
              </div>
              <button
                type="button"
                class="review-open-code"
                onclick={() => void onOpenFile(selectedPath, comparison?.hunks[0]?.new_start ?? 1)}
              >Open in Code</button>
            </div>
          {/if}
        </header>

        {#if loading}
          <div class="review-loading">
            <LoaderCircle size={14} class="animate-spin" />
            <span>Preparing comparison…</span>
          </div>
        {:else if error}
          <div class="review-error">
            <p>Couldn’t prepare this comparison.</p>
            <span>{error}</span>
          </div>
        {:else if comparison?.binary}
          <div class="review-binary">
            <span class="review-binary-icon"><FileQuestion size={22} strokeWidth={1.5} /></span>
            <div>
              <p class="review-binary-title">{binaryMessage(comparison.path).title}</p>
              <p class="review-binary-copy">{binaryMessage(comparison.path).copy}</p>
            </div>
            <dl class="review-binary-facts">
              <div>
                <dt>Before</dt>
                <dd>{comparison.baseline.exists ? formatBytes(comparison.baseline.byte_size) : "Not present"}</dd>
              </div>
              <div>
                <dt>After</dt>
                <dd>{comparison.reviewed.exists ? formatBytes(comparison.reviewed.byte_size) : "Removed"}</dd>
              </div>
            </dl>
            {#if !comparison.baseline.exists}
              <button
                type="button"
                class="review-restore"
                disabled={busy}
                onclick={() => void restore()}
              ><RotateCcw size={12} />Remove added file…</button>
            {/if}
          </div>
        {:else if comparison && comparison.hunks.length === 0}
          <div class="review-empty">
            <p>No textual differences to show.</p>
          </div>
        {:else if comparison && mode === "inline"}
          <div class="review-diff review-diff--inline">
            {#each comparison.hunks as hunk (`${hunk.old_start}:${hunk.new_start}`)}
              <div class="review-hunk">−{hunk.old_start},{hunk.old_count} +{hunk.new_start},{hunk.new_count}</div>
              {#each hunk.lines as line, index (`${line.old_line ?? ""}:${line.new_line ?? ""}:${index}`)}
                <div class="review-diff-line review-diff-line--{line.kind}">
                  <span>{line.old_line ?? ""}</span>
                  <span>{line.new_line ?? ""}</span>
                  <code>{line.kind === "addition" ? "+" : line.kind === "deletion" ? "−" : " "}{line.content}</code>
                </div>
              {/each}
            {/each}
          </div>
        {:else if comparison}
          <div class="review-diff review-diff--side">
            <div class="review-side-labels"><span>Before</span><span>After</span></div>
            {#each sideRows(comparison) as row (row.key)}
              <div class="review-side-row">
                <div class:review-side-old={row.kind === "deletion" || row.kind === "replacement"}>
                  <span>{row.oldNumber ?? ""}</span><code>{row.oldContent}</code>
                </div>
                <div class:review-side-new={row.kind === "addition" || row.kind === "replacement"}>
                  <span>{row.newNumber ?? ""}</span><code>{row.newContent}</code>
                </div>
              </div>
            {/each}
          </div>
        {/if}

        {#if comparison && !comparison.binary}
          <footer class="review-file-footer">
            <p>The reviewed revision remains available as a recovery point.</p>
            <button
              type="button"
              class="review-restore"
              disabled={busy}
              onclick={() => void restore()}
            ><RotateCcw size={12} />Restore before this change…</button>
          </footer>
        {/if}
      </div>
    </div>
  {/if}
</section>

<style>
  .review-surface {
    color: rgb(var(--color-surface-100));
  }

  .review-summary {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1.5rem;
    padding: 0.35rem 0.15rem 1rem;
  }

  .review-summary-copy {
    min-width: 0;
    max-width: 48rem;
  }

  .review-summary-kicker {
    font-size: 0.6875rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-200));
  }

  .review-summary-text {
    margin-top: 0.2rem;
    font-size: 0.75rem;
    line-height: 1.5;
    color: rgb(var(--color-surface-500));
  }

  .review-signals {
    display: flex;
    flex-shrink: 0;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .review-signal {
    border: 1px solid rgb(var(--color-surface-500) / 0.22);
    border-radius: 999px;
    padding: 0.15rem 0.5rem;
    font-size: 0.5625rem;
    font-weight: 500;
    text-transform: capitalize;
    color: rgb(var(--color-surface-400));
  }

  .review-signal--low,
  .review-signal--passed {
    border-color: rgb(var(--color-success-500) / 0.25);
    color: rgb(var(--color-success-300));
  }

  .review-signal--attention,
  .review-signal--unchecked {
    border-color: rgb(var(--color-warning-500) / 0.25);
    color: rgb(var(--color-warning-300));
  }

  .review-signal--high {
    border-color: rgb(var(--color-error-500) / 0.3);
    color: rgb(var(--color-error-300));
  }

  .review-change-shell {
    display: grid;
    min-height: 26rem;
    overflow: hidden;
    grid-template-columns: 13rem minmax(0, 1fr);
    border: 1px solid rgb(var(--color-surface-500) / 0.26);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-950) / 0.2);
  }

  .review-change-shell--single {
    grid-template-columns: minmax(0, 1fr);
  }

  .review-change-shell--compact {
    min-height: 0;
  }

  .review-file-rail {
    border-right: 1px solid rgb(var(--color-surface-500) / 0.2);
    background: rgb(var(--color-surface-900) / 0.2);
  }

  .review-file-rail-label {
    padding: 0.65rem 0.75rem 0.45rem;
    font-size: 0.5625rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-600));
  }

  .review-file-list {
    max-height: 38rem;
    overflow: auto;
    padding: 0 0.35rem 0.5rem;
  }

  .review-file-row {
    display: flex;
    width: 100%;
    min-width: 0;
    align-items: center;
    gap: 0.5rem;
    border: 0;
    border-radius: 0.4rem;
    background: transparent;
    padding: 0.45rem;
    color: rgb(var(--color-surface-500));
    text-align: left;
  }

  .review-file-row:hover,
  .review-file-row--active {
    background: rgb(var(--color-surface-800) / 0.45);
    color: rgb(var(--color-surface-200));
  }

  .review-file-row-copy {
    display: flex;
    min-width: 0;
    flex: 1;
    flex-direction: column;
  }

  .review-file-row-name {
    overflow: hidden;
    font-size: 0.6875rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-file-row-path,
  .review-file-row-state {
    overflow: hidden;
    font-size: 0.5625rem;
    color: rgb(var(--color-surface-600));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-file-stage {
    min-width: 0;
  }

  .review-file-header {
    display: flex;
    min-height: 2.75rem;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.18);
    padding: 0.5rem 0.75rem;
  }

  .review-file-title {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 0.55rem;
    color: rgb(var(--color-surface-400));
  }

  .review-file-title p {
    overflow: hidden;
    font-size: 0.75rem;
    font-weight: 500;
    color: rgb(var(--color-surface-200));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-file-title span {
    display: block;
    overflow: hidden;
    font-size: 0.5625rem;
    color: rgb(var(--color-surface-600));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .review-file-actions,
  .review-mode {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .review-mode {
    gap: 0;
    padding: 0.15rem;
    border-radius: 0.4rem;
    background: rgb(var(--color-surface-800) / 0.45);
  }

  .review-mode button,
  .review-open-code,
  .review-restore {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    color: rgb(var(--color-surface-500));
  }

  .review-mode button {
    width: 1.6rem;
    height: 1.45rem;
  }

  .review-mode button:hover,
  .review-mode .review-mode-active {
    background: rgb(var(--color-surface-700) / 0.65);
    color: rgb(var(--color-surface-100));
  }

  .review-open-code,
  .review-restore {
    padding: 0.3rem 0.5rem;
    font-size: 0.625rem;
  }

  .review-open-code:hover {
    background: rgb(var(--color-primary-500) / 0.1);
    color: rgb(var(--color-primary-300));
  }

  .review-loading,
  .review-error,
  .review-empty {
    display: flex;
    min-height: 9rem;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 2rem;
    color: rgb(var(--color-surface-500));
    font-size: 0.6875rem;
    text-align: center;
  }

  .review-error {
    flex-direction: column;
    color: rgb(var(--color-error-300));
  }

  .review-error span {
    max-width: 34rem;
    color: rgb(var(--color-surface-500));
  }

  .review-binary {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 1rem;
    padding: 1.5rem;
  }

  .review-binary-icon {
    display: inline-flex;
    width: 2.6rem;
    height: 2.6rem;
    align-items: center;
    justify-content: center;
    border: 1px solid rgb(var(--color-surface-500) / 0.22);
    border-radius: 0.7rem;
    background: rgb(var(--color-surface-800) / 0.35);
    color: rgb(var(--color-surface-400));
  }

  .review-binary-title {
    font-size: 0.75rem;
    font-weight: 500;
    color: rgb(var(--color-surface-200));
  }

  .review-binary-copy {
    margin-top: 0.2rem;
    font-size: 0.625rem;
    line-height: 1.45;
    color: rgb(var(--color-surface-500));
  }

  .review-binary-facts {
    display: flex;
    gap: 1.25rem;
  }

  .review-binary-facts div {
    min-width: 3.5rem;
  }

  .review-binary-facts dt {
    font-size: 0.5625rem;
    color: rgb(var(--color-surface-600));
  }

  .review-binary-facts dd {
    margin-top: 0.1rem;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-300));
  }

  .review-diff {
    max-height: 38rem;
    overflow: auto;
    font-family: var(--font-mono);
    font-size: 0.625rem;
    line-height: 1.15rem;
  }

  .review-hunk {
    position: sticky;
    top: 0;
    padding: 0.1rem 0.65rem;
    background: rgb(var(--color-surface-800) / 0.96);
    color: rgb(var(--color-surface-500));
  }

  .review-diff-line {
    display: grid;
    grid-template-columns: 2.25rem 2.25rem minmax(0, 1fr);
    color: rgb(var(--color-surface-400));
  }

  .review-diff-line > span {
    padding-right: 0.4rem;
    color: rgb(var(--color-surface-600));
    text-align: right;
    user-select: none;
  }

  .review-diff code,
  .review-side-row code {
    padding: 0 0.6rem;
    white-space: pre;
  }

  .review-diff-line--addition,
  .review-side-new {
    background: rgb(var(--color-success-950) / 0.32);
    color: rgb(var(--color-success-100));
  }

  .review-diff-line--deletion,
  .review-side-old {
    background: rgb(var(--color-error-950) / 0.32);
    color: rgb(var(--color-error-100));
  }

  .review-side-labels,
  .review-side-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }

  .review-side-labels {
    position: sticky;
    top: 0;
    z-index: 1;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.2);
    background: rgb(var(--color-surface-900) / 0.98);
    color: rgb(var(--color-surface-600));
    font-size: 0.5625rem;
    text-transform: uppercase;
  }

  .review-side-labels span {
    padding: 0.25rem 0.65rem;
  }

  .review-side-labels span + span,
  .review-side-row > div + div {
    border-left: 1px solid rgb(var(--color-surface-500) / 0.18);
  }

  .review-side-row > div {
    display: grid;
    grid-template-columns: 2.25rem minmax(0, 1fr);
  }

  .review-side-row > div > span {
    color: rgb(var(--color-surface-600));
    text-align: right;
    user-select: none;
  }

  .review-file-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.18);
    padding: 0.45rem 0.65rem;
  }

  .review-file-footer p {
    font-size: 0.5625rem;
    color: rgb(var(--color-surface-600));
  }

  .review-restore {
    color: rgb(var(--color-warning-400));
  }

  .review-restore:hover:not(:disabled) {
    background: rgb(var(--color-warning-500) / 0.08);
    color: rgb(var(--color-warning-300));
  }

  .review-restore:disabled {
    opacity: 0.35;
  }

  @media (max-width: 760px) {
    .review-summary,
    .review-binary {
      align-items: flex-start;
      flex-direction: column;
    }

    .review-change-shell {
      grid-template-columns: minmax(0, 1fr);
    }

    .review-file-rail {
      border-right: 0;
      border-bottom: 1px solid rgb(var(--color-surface-500) / 0.2);
    }

    .review-binary {
      display: flex;
    }
  }
</style>
