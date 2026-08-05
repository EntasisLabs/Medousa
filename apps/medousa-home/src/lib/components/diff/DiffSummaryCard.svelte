<script lang="ts">
  interface PathStat {
    path: string;
    additions?: number;
    deletions?: number;
  }

  interface Props {
    fileCount: number;
    additions: number;
    deletions: number;
    paths: PathStat[];
    onViewAll: () => void;
  }

  let { fileCount, additions, deletions, paths, onViewAll }: Props = $props();

  const visible = $derived(paths.slice(0, 3));
  const moreCount = $derived(Math.max(0, fileCount - visible.length));

  function fileLabel(path: string): string {
    return path.split("/").at(-1) || path;
  }
</script>

<button type="button" class="diff-summary-card" onclick={onViewAll}>
  <div class="diff-summary-header">
    <span class="diff-summary-kicker">Diff</span>
    <span class="diff-summary-stats">
      {fileCount} {fileCount === 1 ? "file" : "files"}
      {#if additions > 0 || deletions > 0}
        ·
        <span class="diff-add">+{additions}</span>
        <span class="diff-del">−{deletions}</span>
      {/if}
    </span>
  </div>

  {#if visible.length > 0}
    <ul class="diff-summary-paths">
      {#each visible as entry (entry.path)}
        <li>
          <span class="diff-summary-name">{fileLabel(entry.path)}</span>
          {#if typeof entry.additions === "number" || typeof entry.deletions === "number"}
            <span class="diff-summary-line-stats">
              {#if typeof entry.additions === "number"}
                <span class="diff-add">+{entry.additions}</span>
              {/if}
              {#if typeof entry.deletions === "number"}
                <span class="diff-del">−{entry.deletions}</span>
              {/if}
            </span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  {#if moreCount > 0}
    <span class="diff-summary-more">View {moreCount} more {moreCount === 1 ? "file" : "files"}</span>
  {:else}
    <span class="diff-summary-more">View all</span>
  {/if}
</button>

<style>
  .diff-summary-card {
    display: flex;
    width: 100%;
    flex-direction: column;
    gap: 0.45rem;
    border: 1px solid rgb(var(--color-surface-500) / 0.28);
    border-radius: 0.55rem;
    background: rgb(var(--color-surface-900) / 0.45);
    padding: 0.55rem 0.65rem;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .diff-summary-card:hover {
    border-color: rgb(var(--color-surface-500) / 0.45);
    background: rgb(var(--color-surface-800) / 0.5);
  }

  .diff-summary-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .diff-summary-kicker {
    font-size: 0.625rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: rgb(var(--color-surface-400));
  }

  .diff-summary-stats {
    font-size: 0.625rem;
    font-variant-numeric: tabular-nums;
    color: rgb(var(--color-surface-500));
  }

  .diff-summary-paths {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .diff-summary-paths li {
    display: flex;
    min-width: 0;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .diff-summary-name {
    overflow: hidden;
    font-size: 0.6875rem;
    color: rgb(var(--color-surface-200));
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .diff-summary-line-stats {
    display: inline-flex;
    flex-shrink: 0;
    gap: 0.3rem;
    font-size: 0.5625rem;
    font-variant-numeric: tabular-nums;
  }

  .diff-summary-more {
    font-size: 0.625rem;
    color: rgb(var(--color-primary-300));
  }

  .diff-add {
    color: rgb(var(--color-success-300));
  }

  .diff-del {
    color: rgb(var(--color-error-300));
  }
</style>
