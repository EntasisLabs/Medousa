<script lang="ts">
  import { Columns2, Rows3 } from "@lucide/svelte";
  import DiffFileSection from "./DiffFileSection.svelte";
  import { countStackStats, type DiffFileSection as DiffFileSectionType } from "$lib/diff/diffTypes";

  interface Props {
    files: DiffFileSectionType[];
    mode?: "inline" | "side";
    showJumpList?: boolean;
    busy?: boolean;
    title?: string;
    subtitle?: string;
    onOpenFile?: (path: string, line?: number) => void;
    onRestoreFile?: (path: string) => void;
    onRevertHunk?: (path: string, hunkIndex: number) => void;
    restoreHint?: string;
    restoreLabel?: string;
  }

  let {
    files,
    mode = $bindable<"inline" | "side">("inline"),
    showJumpList,
    busy = false,
    title,
    subtitle,
    onOpenFile,
    onRestoreFile,
    onRevertHunk,
    restoreHint,
    restoreLabel,
  }: Props = $props();

  const stats = $derived(countStackStats(files));
  const jumpListVisible = $derived(showJumpList ?? files.length > 3);

  function fileLabel(path: string): string {
    return path.split("/").at(-1) || path;
  }

  function fileAnchorId(file: DiffFileSectionType, index: number): string {
    return `diff-file-${encodeURIComponent(file.id ?? `${file.path}:${index}`)}`;
  }

  function jumpTo(file: DiffFileSectionType, index: number) {
    const el = document.getElementById(fileAnchorId(file, index));
    el?.scrollIntoView({ behavior: "smooth", block: "start" });
  }
</script>

<section class="diff-stack">
  <header class="diff-stack-header">
    <div class="diff-stack-heading">
      {#if title}
        <h2 class="diff-stack-title">{title}</h2>
      {/if}
      <p class="diff-stack-summary">
        {#if files.length === 0}
          No files changed
        {:else}
          {stats.files} {stats.files === 1 ? "file" : "files"} changed ·
          <span class="diff-add">+{stats.additions}</span>
          <span class="diff-del">−{stats.deletions}</span>
        {/if}
      </p>
      {#if subtitle}
        <p class="diff-stack-subtitle">{subtitle}</p>
      {/if}
    </div>
    <div class="diff-mode" aria-label="Diff layout">
      <button
        type="button"
        class:diff-mode-active={mode === "inline"}
        aria-label="Inline comparison"
        title="Inline comparison"
        onclick={() => (mode = "inline")}
      ><Rows3 size={13} /></button>
      <button
        type="button"
        class:diff-mode-active={mode === "side"}
        aria-label="Side-by-side comparison"
        title="Side-by-side comparison"
        onclick={() => (mode = "side")}
      ><Columns2 size={13} /></button>
    </div>
  </header>

  {#if jumpListVisible && files.length > 0}
    <nav class="diff-jump-list" aria-label="Changed files">
      {#each files as file, index (file.id ?? `${file.path}:${index}`)}
        <button type="button" class="diff-jump-chip" onclick={() => jumpTo(file, index)}>
          {fileLabel(file.path)}
        </button>
      {/each}
    </nav>
  {/if}

  {#if files.length === 0}
    <div class="diff-stack-empty">
      <p>No file changes to review.</p>
    </div>
  {:else}
    <div class="diff-stack-body">
      {#each files as file, index (file.id ?? `${file.path}:${index}`)}
        <div id={fileAnchorId(file, index)} class="diff-stack-item">
          <DiffFileSection
            {file}
            {mode}
            {busy}
            {onOpenFile}
            {restoreHint}
            {restoreLabel}
            onRestore={onRestoreFile ? () => onRestoreFile(file.path) : undefined}
            onRevertHunk={
              onRevertHunk ? (hunkIndex) => onRevertHunk(file.path, hunkIndex) : undefined
            }
          />
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .diff-stack {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    color: rgb(var(--color-surface-100));
  }

  .diff-stack-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.15rem 0.1rem;
  }

  .diff-stack-heading {
    min-width: 0;
  }

  .diff-stack-title {
    font-size: 0.8125rem;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: rgb(var(--color-surface-100));
  }

  .diff-stack-summary {
    margin-top: 0.15rem;
    font-size: 0.6875rem;
    color: rgb(var(--theme-text-tertiary));
  }

  .diff-stack-subtitle {
    margin-top: 0.2rem;
    font-size: 0.625rem;
    line-height: 1.45;
    color: rgb(var(--theme-text-quiet));
  }

  .diff-add {
    color: rgb(var(--theme-success));
  }

  .diff-del {
    color: rgb(var(--theme-error));
    margin-left: 0.25rem;
  }

  .diff-mode {
    display: flex;
    align-items: center;
    gap: 0;
    flex-shrink: 0;
    padding: 0.15rem;
    border-radius: 0.4rem;
    background: rgb(var(--color-surface-800) / 0.45);
  }

  .diff-mode button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.6rem;
    height: 1.45rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    color: rgb(var(--theme-text-quiet));
  }

  .diff-mode button:hover,
  .diff-mode .diff-mode-active {
    background: rgb(var(--color-surface-700) / 0.65);
    color: rgb(var(--color-surface-100));
  }

  .diff-jump-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    padding: 0 0.05rem;
  }

  .diff-jump-chip {
    max-width: 12rem;
    overflow: hidden;
    border: 1px solid rgb(var(--color-surface-500) / 0.22);
    border-radius: 999px;
    background: rgb(var(--color-surface-900) / 0.4);
    padding: 0.15rem 0.55rem;
    color: rgb(var(--theme-text-tertiary));
    font-size: 0.5625rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .diff-jump-chip:hover {
    border-color: rgb(var(--color-surface-500) / 0.4);
    background: rgb(var(--color-surface-800) / 0.55);
    color: rgb(var(--color-surface-200));
  }

  .diff-stack-body {
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }

  .diff-stack-item {
    scroll-margin-top: 0.75rem;
  }

  .diff-stack-empty {
    display: flex;
    min-height: 8rem;
    align-items: center;
    justify-content: center;
    border: 1px solid rgb(var(--color-surface-500) / 0.2);
    border-radius: 0.65rem;
    background: rgb(var(--color-surface-950) / 0.15);
    color: rgb(var(--theme-text-quiet));
    font-size: 0.6875rem;
  }
</style>
