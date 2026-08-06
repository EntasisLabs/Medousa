<script lang="ts">
  import type { DiffHunk, DiffLine } from "$lib/diff/diffTypes";
  import { linesInRange, splitDiffFileLines } from "$lib/diff/diffTypes";

  interface Props {
    hunks: DiffHunk[];
    mode?: "inline" | "side";
    /** Full before-file text for real gap expansion. */
    beforeText?: string | null;
    /** Full after-file text for real gap expansion. */
    afterText?: string | null;
    onRevertHunk?: (hunkIndex: number) => void;
    revertBusy?: boolean;
  }

  let {
    hunks,
    mode = "inline",
    beforeText = null,
    afterText = null,
    onRevertHunk,
    revertBusy = false,
  }: Props = $props();

  /** Expanded gap keys: "lead" | "between:i" */
  let expanded = $state<Record<string, boolean>>({});

  const beforeLines = $derived(splitDiffFileLines(beforeText));
  const afterLines = $derived(splitDiffFileLines(afterText));
  const canExpandReal = $derived(beforeLines.length > 0 || afterLines.length > 0);

  type SideRow = {
    key: string;
    oldNumber?: number | null;
    newNumber?: number | null;
    oldContent: string;
    newContent: string;
    kind: string;
  };

  function gapBefore(hunk: DiffHunk): number {
    if (hunk.new_start <= 1) return 0;
    return hunk.new_start - 1;
  }

  function gapBetween(prev: DiffHunk, next: DiffHunk): number {
    const prevEnd = prev.new_start + prev.new_count;
    return Math.max(0, next.new_start - prevEnd);
  }

  function toggle(key: string) {
    expanded = { ...expanded, [key]: !expanded[key] };
  }

  function leadGapRows(hunk: DiffHunk): {
    after: Array<{ line: number; content: string }>;
    before: Array<{ line: number; content: string }>;
  } {
    const count = gapBefore(hunk);
    if (count <= 0) return { after: [], before: [] };
    const newStart = 1;
    const newEnd = hunk.new_start - 1;
    const oldStart = Math.max(1, hunk.old_start - count);
    const oldEnd = Math.max(oldStart - 1, hunk.old_start - 1);
    return {
      after: linesInRange(afterLines, newStart, newEnd),
      before: linesInRange(beforeLines, oldStart, oldEnd),
    };
  }

  function betweenGapRows(
    prev: DiffHunk,
    next: DiffHunk,
  ): {
    after: Array<{ line: number; content: string }>;
    before: Array<{ line: number; content: string }>;
  } {
    const count = gapBetween(prev, next);
    if (count <= 0) return { after: [], before: [] };
    const newStart = prev.new_start + prev.new_count;
    const newEnd = next.new_start - 1;
    const oldStart = prev.old_start + prev.old_count;
    const oldEnd = next.old_start - 1;
    return {
      after: linesInRange(afterLines, newStart, newEnd),
      before: linesInRange(beforeLines, oldStart, oldEnd),
    };
  }

  function sideRowsForHunk(hunk: DiffHunk): SideRow[] {
    const rows: SideRow[] = [];
    for (let index = 0; index < hunk.lines.length; ) {
      const line = hunk.lines[index]!;
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
      const block: DiffLine[] = [];
      while (index < hunk.lines.length && hunk.lines[index]!.kind !== "context") {
        block.push(hunk.lines[index]!);
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
  }

  function linePrefix(kind: string): string {
    if (kind === "addition") return "+";
    if (kind === "deletion") return "−";
    return " ";
  }
</script>

{#if hunks.length === 0}
  <div class="diff-empty">No textual differences to show.</div>
{:else if mode === "inline"}
  <div class="diff-view diff-view--inline">
    {#each hunks as hunk, hi (`${hunk.old_start}:${hunk.new_start}:${hi}`)}
      {#if hi === 0}
        {@const lead = gapBefore(hunk)}
        {#if lead > 0}
          <button
            type="button"
            class="diff-gap"
            class:diff-gap--expanded={expanded["lead"]}
            onclick={() => toggle("lead")}
          >
            {#if expanded["lead"]}
              {#if canExpandReal}
                {@const rows = leadGapRows(hunk)}
                <span class="diff-gap-expanded-block">
                  {#each rows.after as row (row.line)}
                    <span class="diff-line diff-line--context">
                      <span class="diff-gutter"></span>
                      <span class="diff-gutter">{row.line}</span>
                      <code> {row.content}</code>
                    </span>
                  {:else}
                    <span class="diff-gap-expanded">{lead} unmodified lines</span>
                  {/each}
                </span>
              {:else}
                <span class="diff-gap-expanded">{lead} unmodified lines</span>
              {/if}
            {:else}
              {lead} unmodified lines
            {/if}
          </button>
        {/if}
      {:else}
        {@const gap = gapBetween(hunks[hi - 1]!, hunk)}
        {#if gap > 0}
          {@const key = `between:${hi}`}
          <button
            type="button"
            class="diff-gap"
            class:diff-gap--expanded={expanded[key]}
            onclick={() => toggle(key)}
          >
            {#if expanded[key]}
              {#if canExpandReal}
                {@const rows = betweenGapRows(hunks[hi - 1]!, hunk)}
                <span class="diff-gap-expanded-block">
                  {#each rows.after as row (row.line)}
                    <span class="diff-line diff-line--context">
                      <span class="diff-gutter"></span>
                      <span class="diff-gutter">{row.line}</span>
                      <code> {row.content}</code>
                    </span>
                  {:else}
                    <span class="diff-gap-expanded">{gap} unmodified lines</span>
                  {/each}
                </span>
              {:else}
                <span class="diff-gap-expanded">{gap} unmodified lines</span>
              {/if}
            {:else}
              {gap} unmodified lines
            {/if}
          </button>
        {/if}
      {/if}

      <div class="diff-hunk-meta">
        <span>−{hunk.old_start},{hunk.old_count} +{hunk.new_start},{hunk.new_count}</span>
        {#if onRevertHunk}
          <button
            type="button"
            class="diff-hunk-revert"
            disabled={revertBusy}
            onclick={(event) => {
              event.stopPropagation();
              onRevertHunk(hi);
            }}
          >Revert hunk</button>
        {/if}
      </div>
      {#each hunk.lines as line, index (`${line.old_line ?? ""}:${line.new_line ?? ""}:${index}`)}
        <div class="diff-line diff-line--{line.kind}">
          <span class="diff-gutter">{line.old_line ?? ""}</span>
          <span class="diff-gutter">{line.new_line ?? ""}</span>
          <code>{linePrefix(line.kind)}{line.content}</code>
        </div>
      {/each}
    {/each}
  </div>
{:else}
  <div class="diff-view diff-view--side">
    <div class="diff-side-labels"><span>Before</span><span>After</span></div>
    {#each hunks as hunk, hi (`${hunk.old_start}:${hunk.new_start}:side:${hi}`)}
      {#if hi === 0}
        {@const lead = gapBefore(hunk)}
        {#if lead > 0}
          <button
            type="button"
            class="diff-gap diff-gap--side"
            class:diff-gap--expanded={expanded["lead"]}
            onclick={() => toggle("lead")}
          >
            {#if expanded["lead"]}
              {#if canExpandReal}
                {@const rows = leadGapRows(hunk)}
                <span class="diff-gap-expanded-block diff-gap-expanded-block--side">
                  {#each rows.after as row, i (row.line)}
                    <span class="diff-side-row">
                      <div>
                        <span class="diff-gutter">{rows.before[i]?.line ?? ""}</span>
                        <code>{rows.before[i]?.content ?? row.content}</code>
                      </div>
                      <div>
                        <span class="diff-gutter">{row.line}</span>
                        <code>{row.content}</code>
                      </div>
                    </span>
                  {:else}
                    <span class="diff-gap-expanded">{lead} unmodified lines</span>
                  {/each}
                </span>
              {:else}
                <span class="diff-gap-expanded">{lead} unmodified lines</span>
              {/if}
            {:else}
              {lead} unmodified lines
            {/if}
          </button>
        {/if}
      {:else}
        {@const gap = gapBetween(hunks[hi - 1]!, hunk)}
        {#if gap > 0}
          {@const key = `between:${hi}`}
          <button
            type="button"
            class="diff-gap diff-gap--side"
            class:diff-gap--expanded={expanded[key]}
            onclick={() => toggle(key)}
          >
            {#if expanded[key]}
              {#if canExpandReal}
                {@const rows = betweenGapRows(hunks[hi - 1]!, hunk)}
                <span class="diff-gap-expanded-block diff-gap-expanded-block--side">
                  {#each rows.after as row, i (row.line)}
                    <span class="diff-side-row">
                      <div>
                        <span class="diff-gutter">{rows.before[i]?.line ?? ""}</span>
                        <code>{rows.before[i]?.content ?? row.content}</code>
                      </div>
                      <div>
                        <span class="diff-gutter">{row.line}</span>
                        <code>{row.content}</code>
                      </div>
                    </span>
                  {:else}
                    <span class="diff-gap-expanded">{gap} unmodified lines</span>
                  {/each}
                </span>
              {:else}
                <span class="diff-gap-expanded">{gap} unmodified lines</span>
              {/if}
            {:else}
              {gap} unmodified lines
            {/if}
          </button>
        {/if}
      {/if}

      {#if onRevertHunk}
        <div class="diff-hunk-meta diff-hunk-meta--side">
          <span>−{hunk.old_start},{hunk.old_count} +{hunk.new_start},{hunk.new_count}</span>
          <button
            type="button"
            class="diff-hunk-revert"
            disabled={revertBusy}
            onclick={() => onRevertHunk(hi)}
          >Revert hunk</button>
        </div>
      {/if}

      {#each sideRowsForHunk(hunk) as row (row.key)}
        <div class="diff-side-row">
          <div class:diff-side-old={row.kind === "deletion" || row.kind === "replacement"}>
            <span class="diff-gutter">{row.oldNumber ?? ""}</span>
            <code>{row.oldContent}</code>
          </div>
          <div class:diff-side-new={row.kind === "addition" || row.kind === "replacement"}>
            <span class="diff-gutter">{row.newNumber ?? ""}</span>
            <code>{row.newContent}</code>
          </div>
        </div>
      {/each}
    {/each}
  </div>
{/if}

<style>
  .diff-view {
    overflow: auto;
    font-family: var(--font-mono);
    font-size: 0.625rem;
    line-height: 1.15rem;
  }

  .diff-empty {
    display: flex;
    min-height: 4rem;
    align-items: center;
    justify-content: center;
    padding: 1.25rem;
    color: rgb(var(--theme-text-quiet));
    font-size: 0.6875rem;
  }

  .diff-hunk-meta {
    position: sticky;
    top: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.1rem 0.65rem;
    background: rgb(var(--color-surface-800) / 0.96);
    color: rgb(var(--theme-text-quiet));
  }

  .diff-hunk-meta--side {
    grid-column: 1 / -1;
  }

  .diff-hunk-revert {
    border: 0;
    border-radius: 0.25rem;
    background: transparent;
    padding: 0.1rem 0.35rem;
    color: rgb(var(--theme-warning));
    font-family: inherit;
    font-size: 0.5625rem;
    cursor: pointer;
  }

  .diff-hunk-revert:hover:not(:disabled) {
    background: rgb(var(--color-warning-500) / 0.1);
  }

  .diff-hunk-revert:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .diff-line {
    display: grid;
    grid-template-columns: 2.25rem 2.25rem minmax(0, 1fr);
    color: rgb(var(--theme-text-tertiary));
  }

  .diff-gutter {
    padding-right: 0.4rem;
    color: rgb(var(--theme-text-faint));
    text-align: right;
    user-select: none;
  }

  .diff-view code {
    padding: 0 0.6rem;
    white-space: pre;
  }

  .diff-line--addition,
  .diff-side-new {
    background: rgb(var(--color-success-950) / 0.32);
    color: rgb(var(--theme-success));
  }

  .diff-line--deletion,
  .diff-side-old {
    background: rgb(var(--color-error-950) / 0.32);
    color: rgb(var(--theme-error));
  }

  .diff-gap {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: center;
    border: 0;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.12);
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.12);
    background: rgb(var(--color-surface-900) / 0.55);
    padding: 0.2rem 0.65rem;
    color: rgb(var(--theme-text-quiet));
    font-family: inherit;
    font-size: 0.5625rem;
    cursor: pointer;
  }

  .diff-gap:hover {
    background: rgb(var(--color-surface-800) / 0.65);
    color: rgb(var(--theme-text-secondary));
  }

  .diff-gap--expanded {
    min-height: 2.25rem;
    background: rgb(var(--color-surface-950) / 0.35);
    align-items: stretch;
    justify-content: stretch;
    padding: 0;
  }

  .diff-gap-expanded {
    color: rgb(var(--theme-text-faint));
    font-style: italic;
    padding: 0.2rem 0.65rem;
  }

  .diff-gap-expanded-block {
    display: flex;
    width: 100%;
    flex-direction: column;
    text-align: left;
  }

  .diff-gap-expanded-block--side {
    display: block;
  }

  .diff-gap-expanded-block .diff-line {
    pointer-events: none;
  }

  .diff-side-labels,
  .diff-side-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }

  .diff-side-labels {
    position: sticky;
    top: 0;
    z-index: 1;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.2);
    background: rgb(var(--color-surface-900) / 0.98);
    color: rgb(var(--theme-text-faint));
    font-size: 0.5625rem;
    text-transform: uppercase;
  }

  .diff-side-labels span {
    padding: 0.25rem 0.65rem;
  }

  .diff-side-labels span + span,
  .diff-side-row > div + div {
    border-left: 1px solid rgb(var(--color-surface-500) / 0.18);
  }

  .diff-side-row > div {
    display: grid;
    grid-template-columns: 2.25rem minmax(0, 1fr);
    min-width: 0;
  }

  .diff-gap--side {
    grid-column: 1 / -1;
  }
</style>
