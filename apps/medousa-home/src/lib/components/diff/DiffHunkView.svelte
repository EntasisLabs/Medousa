<script lang="ts">
  import type { DiffHunk } from "$lib/diff/diffTypes";
  import { linesInRange, splitDiffFileLines } from "$lib/diff/diffTypes";
  import { sideRowsForHunk, wordDiffParts } from "$lib/diff/wordDiff";
  import DiffCodeLine from "./DiffCodeLine.svelte";
  import { ChevronDown, ChevronUp, MessageSquarePlus } from "@lucide/svelte";

  interface Props {
    hunks: DiffHunk[];
    mode?: "inline" | "side";
    /** Full before-file text for real gap expansion. */
    beforeText?: string | null;
    /** Full after-file text for real gap expansion. */
    afterText?: string | null;
    languageHint?: string | null;
    density?: "comfortable" | "compact";
    wrap?: boolean;
    onRevertHunk?: (hunkIndex: number) => void;
    revertBusy?: boolean;
    /** When set, shows a gutter comment affordance on hover. */
    onComment?: (input: {
      side: "new" | "old";
      line: number;
      content: string;
    }) => void;
  }

  let {
    hunks,
    mode = "inline",
    beforeText = null,
    afterText = null,
    languageHint = null,
    density = "comfortable",
    wrap = false,
    onRevertHunk,
    revertBusy = false,
    onComment,
  }: Props = $props();

  let expandedDeleteHunks = $state<Set<string>>(new Set());

  function hunkKey(hunk: DiffHunk, hi: number): string {
    return `${hunk.old_start}:${hunk.new_start}:${hi}`;
  }

  function isPureDeletion(hunk: DiffHunk): boolean {
    return hunk.lines.length >= 4 && hunk.lines.every((line) => line.kind === "deletion");
  }

  function deletionCollapsed(hunk: DiffHunk, hi: number): boolean {
    return isPureDeletion(hunk) && !expandedDeleteHunks.has(hunkKey(hunk, hi));
  }

  function expandDeletion(hunk: DiffHunk, hi: number) {
    const next = new Set(expandedDeleteHunks);
    next.add(hunkKey(hunk, hi));
    expandedDeleteHunks = next;
  }

  /** Gap reveal: lines shown from the start and end of each unmodified region. */
  let gapReveal = $state<Record<string, { head: number; tail: number }>>({});

  const beforeLines = $derived(splitDiffFileLines(beforeText));
  const afterLines = $derived(splitDiffFileLines(afterText));
  const canExpandReal = $derived(beforeLines.length > 0 || afterLines.length > 0);

  const GAP_STEP = 20;

  function gapBefore(hunk: DiffHunk): number {
    if (hunk.new_start <= 1) return 0;
    return hunk.new_start - 1;
  }

  function gapBetween(prev: DiffHunk, next: DiffHunk): number {
    const prevEnd = prev.new_start + prev.new_count;
    return Math.max(0, next.new_start - prevEnd);
  }

  function revealFor(key: string): { head: number; tail: number } {
    return gapReveal[key] ?? { head: 0, tail: 0 };
  }

  function setReveal(key: string, next: { head: number; tail: number }) {
    gapReveal = { ...gapReveal, [key]: next };
  }

  function expandGapHead(key: string, total: number) {
    const current = revealFor(key);
    const remaining = Math.max(0, total - current.tail);
    const head = Math.min(remaining, current.head + GAP_STEP);
    setReveal(key, { head, tail: current.tail });
  }

  function expandGapTail(key: string, total: number) {
    const current = revealFor(key);
    const remaining = Math.max(0, total - current.head);
    const tail = Math.min(remaining, current.tail + GAP_STEP);
    setReveal(key, { head: current.head, tail });
  }

  function expandGapAll(key: string, total: number) {
    setReveal(key, { head: total, tail: 0 });
  }

  function collapseGap(key: string) {
    setReveal(key, { head: 0, tail: 0 });
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

  function lineMarker(kind: string): string {
    if (kind === "addition") return "+";
    if (kind === "deletion") return "−";
    return " ";
  }

  function sideMarker(kind: string, side: "old" | "new"): string {
    if (side === "old" && (kind === "deletion" || kind === "replacement")) return "−";
    if (side === "new" && (kind === "addition" || kind === "replacement")) return "+";
    return " ";
  }

  function toneFor(kind: string): "add" | "del" | null {
    if (kind === "addition") return "add";
    if (kind === "deletion") return "del";
    return null;
  }

  function inlineWordParts(kind: string, content: string, peer?: string | null) {
    if ((kind !== "addition" && kind !== "deletion") || !peer) return null;
    const parts = wordDiffParts(
      kind === "deletion" ? content : peer,
      kind === "addition" ? content : peer,
    );
    return kind === "deletion" ? parts.before : parts.after;
  }

  function peerForInline(hunkLines: DiffHunk["lines"], index: number): string | null {
    const line = hunkLines[index]!;
    if (line.kind === "deletion") {
      for (let i = index + 1; i < hunkLines.length; i += 1) {
        if (hunkLines[i]!.kind === "context") break;
        if (hunkLines[i]!.kind === "addition") return hunkLines[i]!.content;
      }
    }
    if (line.kind === "addition") {
      for (let i = index - 1; i >= 0; i -= 1) {
        if (hunkLines[i]!.kind === "context") break;
        if (hunkLines[i]!.kind === "deletion") return hunkLines[i]!.content;
      }
    }
    return null;
  }

  function hunkLineLabel(hunk: DiffHunk): string {
    const start = hunk.new_start;
    const end = Math.max(start, hunk.new_start + Math.max(0, hunk.new_count) - 1);
    if (hunk.new_count <= 0) {
      const oldStart = hunk.old_start;
      const oldEnd = Math.max(oldStart, hunk.old_start + Math.max(0, hunk.old_count) - 1);
      return oldStart === oldEnd ? `Line ${oldStart}` : `Lines ${oldStart}–${oldEnd}`;
    }
    return start === end ? `Line ${start}` : `Lines ${start}–${end}`;
  }

  function splitGapRows<T>(
    rows: T[],
    reveal: { head: number; tail: number },
  ): { head: T[]; middle: number; tail: T[] } {
    const total = rows.length;
    let head = Math.min(reveal.head, total);
    let tail = Math.min(reveal.tail, Math.max(0, total - head));
    if (head + tail >= total) {
      return { head: rows, middle: 0, tail: [] };
    }
    return {
      head: rows.slice(0, head),
      middle: total - head - tail,
      tail: tail > 0 ? rows.slice(total - tail) : [],
    };
  }
</script>

{#snippet gapControls(key: string, total: number, middle: number, fullyOpen: boolean)}
  <div class="diff-gap" class:diff-gap--side={mode === "side"}>
    {#if fullyOpen}
      <button type="button" class="diff-gap-action" onclick={() => collapseGap(key)}>
        Collapse unmodified lines
      </button>
    {:else}
      <button
        type="button"
        class="diff-gap-action"
        title="Expand {Math.min(GAP_STEP, middle)} lines above"
        aria-label="Expand up"
        onclick={() => expandGapHead(key, total)}
      ><ChevronUp size={12} /><span>{Math.min(GAP_STEP, middle)}</span></button>
      <button
        type="button"
        class="diff-gap-action diff-gap-action--all"
        onclick={() => expandGapAll(key, total)}
      >{middle} unmodified lines</button>
      <button
        type="button"
        class="diff-gap-action"
        title="Expand {Math.min(GAP_STEP, middle)} lines below"
        aria-label="Expand down"
        onclick={() => expandGapTail(key, total)}
      ><span>{Math.min(GAP_STEP, middle)}</span><ChevronDown size={12} /></button>
    {/if}
  </div>
{/snippet}

{#snippet inlineContextRow(line: number, content: string)}
  <div class="diff-line diff-line--context" data-diff-line={line}>
    <span class="diff-comment-slot"></span>
    <span class="diff-gutter diff-gutter--old"></span>
    <span class="diff-gutter diff-gutter--new">{line}</span>
    <span class="diff-marker" aria-hidden="true"> </span>
    <DiffCodeLine {content} {languageHint} {wrap} />
  </div>
{/snippet}

{#snippet sideContextRow(
  beforeLine: number | undefined,
  beforeContent: string,
  afterLine: number,
  afterContent: string,
)}
  <div class="diff-side-row" data-diff-line={afterLine}>
    <div>
      <span class="diff-comment-slot"></span>
      <span class="diff-gutter diff-gutter--old">{beforeLine ?? ""}</span>
      <span class="diff-marker" aria-hidden="true"> </span>
      <DiffCodeLine content={beforeContent} {languageHint} {wrap} />
    </div>
    <div>
      <span class="diff-comment-slot"></span>
      <span class="diff-gutter diff-gutter--new">{afterLine}</span>
      <span class="diff-marker" aria-hidden="true"> </span>
      <DiffCodeLine content={afterContent} {languageHint} {wrap} />
    </div>
  </div>
{/snippet}

{#if hunks.length === 0}
  <div class="diff-empty">No textual differences to show.</div>
{:else if mode === "inline"}
  <div
    class="diff-view diff-view--inline"
    class:diff-view--compact={density === "compact"}
    class:diff-view--wrap={wrap}
  >
    {#each hunks as hunk, hi (`${hunk.old_start}:${hunk.new_start}:${hi}`)}
      {#if deletionCollapsed(hunk, hi)}
        <button
          type="button"
          class="diff-delete-collapse"
          onclick={() => expandDeletion(hunk, hi)}
        >
          Removed {hunk.lines.length} lines · {hunkLineLabel(hunk)} — show
        </button>
      {:else}
      {#if hi === 0}
        {@const lead = gapBefore(hunk)}
        {#if lead > 0}
          {@const key = "lead"}
          {@const reveal = revealFor(key)}
          {@const rows = canExpandReal ? leadGapRows(hunk).after : []}
          {@const split = splitGapRows(rows.length ? rows : Array.from({ length: lead }, (_, i) => ({
            line: i + 1,
            content: "",
          })), reveal)}
          {#each split.head as row (row.line)}
            {#if canExpandReal && row.content !== undefined}
              {@render inlineContextRow(row.line, "content" in row ? row.content : "")}
            {/if}
          {/each}
          {#if split.middle > 0 || !canExpandReal}
            {@render gapControls(key, lead, canExpandReal ? split.middle : lead, false)}
          {:else if reveal.head > 0 || reveal.tail > 0}
            {@render gapControls(key, lead, 0, true)}
          {/if}
          {#each split.tail as row (row.line)}
            {#if canExpandReal}
              {@render inlineContextRow(row.line, row.content)}
            {/if}
          {/each}
        {/if}
      {:else}
        {@const gap = gapBetween(hunks[hi - 1]!, hunk)}
        {#if gap > 0}
          {@const key = `between:${hi}`}
          {@const reveal = revealFor(key)}
          {@const rows = canExpandReal ? betweenGapRows(hunks[hi - 1]!, hunk).after : []}
          {@const split = splitGapRows(rows.length ? rows : Array.from({ length: gap }, (_, i) => ({
            line: i + 1,
            content: "",
          })), reveal)}
          {#each split.head as row (row.line)}
            {#if canExpandReal}
              {@render inlineContextRow(row.line, row.content)}
            {/if}
          {/each}
          {#if split.middle > 0 || !canExpandReal}
            {@render gapControls(key, gap, canExpandReal ? split.middle : gap, false)}
          {:else if reveal.head > 0 || reveal.tail > 0}
            {@render gapControls(key, gap, 0, true)}
          {/if}
          {#each split.tail as row (row.line)}
            {#if canExpandReal}
              {@render inlineContextRow(row.line, row.content)}
            {/if}
          {/each}
        {/if}
      {/if}

      <div class="diff-hunk-meta">
        <span>{hunkLineLabel(hunk)}</span>
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
        {@const peer = peerForInline(hunk.lines, index)}
        <div class="diff-line diff-line--{line.kind}" data-diff-line={line.new_line ?? line.old_line ?? ""}>
          <span class="diff-comment-slot">
            {#if onComment && (line.new_line || line.old_line)}
              <button
                type="button"
                class="diff-line-comment"
                title="Add comment"
                aria-label="Add comment on line {line.new_line ?? line.old_line}"
                onclick={() =>
                  onComment({
                    side: line.new_line ? "new" : "old",
                    line: (line.new_line ?? line.old_line)!,
                    content: line.content,
                  })}
              ><MessageSquarePlus size={11} /></button>
            {/if}
          </span>
          <span class="diff-gutter diff-gutter--old">{line.old_line ?? ""}</span>
          <span class="diff-gutter diff-gutter--new">{line.new_line ?? ""}</span>
          <span class="diff-marker" aria-hidden="true">{lineMarker(line.kind)}</span>
          <DiffCodeLine
            content={line.content}
            {languageHint}
            {wrap}
            parts={inlineWordParts(line.kind, line.content, peer)}
            tone={toneFor(line.kind)}
          />
        </div>
      {/each}
      {/if}
    {/each}
  </div>
{:else}
  <div
    class="diff-view diff-view--side"
    class:diff-view--compact={density === "compact"}
    class:diff-view--wrap={wrap}
  >
    <div class="diff-side-labels"><span>Before</span><span>After</span></div>
    {#each hunks as hunk, hi (`${hunk.old_start}:${hunk.new_start}:side:${hi}`)}
      {#if deletionCollapsed(hunk, hi)}
        <button
          type="button"
          class="diff-delete-collapse diff-delete-collapse--side"
          onclick={() => expandDeletion(hunk, hi)}
        >
          Removed {hunk.lines.length} lines · {hunkLineLabel(hunk)} — show
        </button>
      {:else}
      {#if hi === 0}
        {@const lead = gapBefore(hunk)}
        {#if lead > 0}
          {@const key = "lead"}
          {@const reveal = revealFor(key)}
          {@const full = canExpandReal ? leadGapRows(hunk) : { after: [], before: [] }}
          {@const split = splitGapRows(
            full.after.length
              ? full.after.map((row, i) => ({
                  after: row,
                  before: full.before[i],
                }))
              : Array.from({ length: lead }, (_, i) => ({
                  after: { line: i + 1, content: "" },
                  before: undefined as { line: number; content: string } | undefined,
                })),
            reveal,
          )}
          {#each split.head as row (`h-${row.after.line}`)}
            {#if canExpandReal}
              {@render sideContextRow(
                row.before?.line,
                row.before?.content ?? row.after.content,
                row.after.line,
                row.after.content,
              )}
            {/if}
          {/each}
          {#if split.middle > 0 || !canExpandReal}
            {@render gapControls(key, lead, canExpandReal ? split.middle : lead, false)}
          {:else if reveal.head > 0 || reveal.tail > 0}
            {@render gapControls(key, lead, 0, true)}
          {/if}
          {#each split.tail as row (`t-${row.after.line}`)}
            {#if canExpandReal}
              {@render sideContextRow(
                row.before?.line,
                row.before?.content ?? row.after.content,
                row.after.line,
                row.after.content,
              )}
            {/if}
          {/each}
        {/if}
      {:else}
        {@const gap = gapBetween(hunks[hi - 1]!, hunk)}
        {#if gap > 0}
          {@const key = `between:${hi}`}
          {@const reveal = revealFor(key)}
          {@const full = canExpandReal ? betweenGapRows(hunks[hi - 1]!, hunk) : { after: [], before: [] }}
          {@const split = splitGapRows(
            full.after.length
              ? full.after.map((row, i) => ({
                  after: row,
                  before: full.before[i],
                }))
              : Array.from({ length: gap }, (_, i) => ({
                  after: { line: i + 1, content: "" },
                  before: undefined as { line: number; content: string } | undefined,
                })),
            reveal,
          )}
          {#each split.head as row (`h-${row.after.line}`)}
            {#if canExpandReal}
              {@render sideContextRow(
                row.before?.line,
                row.before?.content ?? row.after.content,
                row.after.line,
                row.after.content,
              )}
            {/if}
          {/each}
          {#if split.middle > 0 || !canExpandReal}
            {@render gapControls(key, gap, canExpandReal ? split.middle : gap, false)}
          {:else if reveal.head > 0 || reveal.tail > 0}
            {@render gapControls(key, gap, 0, true)}
          {/if}
          {#each split.tail as row (`t-${row.after.line}`)}
            {#if canExpandReal}
              {@render sideContextRow(
                row.before?.line,
                row.before?.content ?? row.after.content,
                row.after.line,
                row.after.content,
              )}
            {/if}
          {/each}
        {/if}
      {/if}

      <div class="diff-hunk-meta diff-hunk-meta--side">
        <span>{hunkLineLabel(hunk)}</span>
        {#if onRevertHunk}
          <button
            type="button"
            class="diff-hunk-revert"
            disabled={revertBusy}
            onclick={() => onRevertHunk(hi)}
          >Revert hunk</button>
        {/if}
      </div>

      {#each sideRowsForHunk(`${hunk.old_start}:${hunk.new_start}:${hi}`, hunk.lines) as row (row.key)}
        <div class="diff-side-row" data-diff-line={row.newNumber ?? row.oldNumber ?? ""}>
          <div class:diff-side-old={row.kind === "deletion" || row.kind === "replacement"}>
            <span class="diff-comment-slot"></span>
            <span class="diff-gutter diff-gutter--old">{row.oldNumber ?? ""}</span>
            <span class="diff-marker" aria-hidden="true">{sideMarker(row.kind, "old")}</span>
            <DiffCodeLine
              content={row.oldContent}
              {languageHint}
              {wrap}
              parts={row.oldParts}
              tone={row.kind === "deletion" || row.kind === "replacement" ? "del" : null}
            />
          </div>
          <div class:diff-side-new={row.kind === "addition" || row.kind === "replacement"}>
            <span class="diff-comment-slot">
              {#if onComment && (row.newNumber || row.oldNumber)}
                <button
                  type="button"
                  class="diff-line-comment"
                  title="Add comment"
                  aria-label="Add comment on line {row.newNumber ?? row.oldNumber}"
                  onclick={() =>
                    onComment({
                      side: row.newNumber ? "new" : "old",
                      line: (row.newNumber ?? row.oldNumber)!,
                      content: row.newContent || row.oldContent,
                    })}
                ><MessageSquarePlus size={11} /></button>
              {/if}
            </span>
            <span class="diff-gutter diff-gutter--new">{row.newNumber ?? ""}</span>
            <span class="diff-marker" aria-hidden="true">{sideMarker(row.kind, "new")}</span>
            <DiffCodeLine
              content={row.newContent}
              {languageHint}
              {wrap}
              parts={row.newParts}
              tone={row.kind === "addition" || row.kind === "replacement" ? "add" : null}
            />
          </div>
        </div>
      {/each}
      {/if}
    {/each}
  </div>
{/if}

<style>
  .diff-view {
    overflow: auto;
    font-family: var(--font-mono);
    font-size: 0.78125rem;
    line-height: 1.35rem;
  }

  .diff-view--compact {
    font-size: 0.75rem;
    line-height: 1.2rem;
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

  .diff-delete-collapse {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 0.35rem;
    margin: 0.15rem 0;
    padding: 0.35rem 0.75rem;
    border: 1px dashed rgb(var(--theme-border) / 0.55);
    border-radius: 0.35rem;
    background: rgb(var(--theme-error) / 0.06);
    color: rgb(var(--theme-text-quiet));
    font: inherit;
    font-size: 0.6875rem;
    text-align: left;
    cursor: pointer;
  }

  .diff-delete-collapse:hover {
    background: rgb(var(--theme-error) / 0.1);
    color: rgb(var(--theme-text));
  }

  .diff-delete-collapse--side {
    grid-column: 1 / -1;
  }

  .diff-hunk-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
    padding: 0.15rem 0.65rem;
    background: rgb(var(--color-surface-800) / 0.55);
    color: rgb(var(--theme-text-quiet));
    font-size: 0.6875rem;
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
    font-size: 0.6875rem;
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
    grid-template-columns: 1.5rem 2.25rem 2.25rem 1.1rem minmax(0, 1fr);
    background: rgb(var(--color-surface-950));
    color: rgb(var(--theme-text-tertiary));
  }

  .diff-comment-slot,
  .diff-gutter,
  .diff-marker {
    position: sticky;
    left: 0;
    z-index: 1;
    background: inherit;
  }

  .diff-gutter--old {
    left: 1.5rem;
  }

  .diff-gutter--new {
    left: 3.75rem;
  }

  .diff-marker {
    left: 6rem;
  }

  .diff-side-row > div .diff-gutter--old,
  .diff-side-row > div .diff-gutter--new {
    left: 1.5rem;
  }

  .diff-side-row > div .diff-marker {
    left: 3.75rem;
  }

  .diff-comment-slot {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
  }

  .diff-gutter {
    padding-right: 0.4rem;
    text-align: right;
    user-select: none;
  }

  .diff-gutter--old {
    color: rgb(var(--theme-text-quiet));
  }

  .diff-gutter--new {
    color: rgb(var(--theme-text-secondary));
  }

  .diff-marker {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-left: 3px solid transparent;
    color: rgb(var(--theme-text-quiet));
    user-select: none;
  }

  .diff-line--addition,
  .diff-side-new {
    background: rgb(var(--syn-addition-bg) / 0.1);
  }

  .diff-line--deletion,
  .diff-side-old {
    background: rgb(var(--syn-deletion-bg) / 0.07);
  }

  .diff-line--addition .diff-marker,
  .diff-side-new .diff-marker {
    border-left-color: color-mix(
      in srgb,
      rgb(var(--syn-addition-bg)) 70%,
      transparent
    );
    color: color-mix(
      in srgb,
      rgb(var(--theme-success)) 65%,
      rgb(var(--theme-text-secondary))
    );
  }

  .diff-line--deletion .diff-marker,
  .diff-side-old .diff-marker {
    border-left-color: color-mix(
      in srgb,
      rgb(var(--syn-deletion-bg)) 65%,
      transparent
    );
    color: color-mix(
      in srgb,
      rgb(var(--theme-error)) 65%,
      rgb(var(--theme-text-secondary))
    );
  }

  :global(html.dark) .diff-line--addition,
  :global(html.dark) .diff-side-new {
    background: rgb(var(--syn-addition-bg) / 0.055);
  }

  :global(html.dark) .diff-line--deletion,
  :global(html.dark) .diff-side-old {
    background: rgb(var(--syn-deletion-bg) / 0.04);
  }

  :global(html.dark) .diff-line--addition .diff-marker,
  :global(html.dark) .diff-side-new .diff-marker {
    border-left-color: rgb(var(--syn-addition-bg) / 0.55);
  }

  :global(html.dark) .diff-line--deletion .diff-marker,
  :global(html.dark) .diff-side-old .diff-marker {
    border-left-color: rgb(var(--syn-deletion-bg) / 0.5);
  }

  .diff-line-comment {
    display: none;
    align-items: center;
    justify-content: center;
    width: 1.15rem;
    height: 1.15rem;
    border: 0;
    border-radius: 0.25rem;
    background: rgb(var(--color-surface-800) / 0.9);
    color: rgb(var(--theme-text-quiet));
    cursor: pointer;
  }

  .diff-line:hover .diff-line-comment,
  .diff-side-row:hover .diff-line-comment {
    display: inline-flex;
  }

  .diff-line-comment:hover {
    color: rgb(var(--theme-link));
    background: rgb(var(--color-primary-500) / 0.12);
  }

  .diff-gap {
    display: flex;
    width: 100%;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    border-top: 1px solid rgb(var(--color-surface-500) / 0.12);
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.12);
    background: rgb(var(--color-surface-900) / 0.55);
    padding: 0.15rem 0.45rem;
    color: rgb(var(--theme-text-quiet));
    font-family: inherit;
    font-size: 0.6875rem;
  }

  .diff-gap-action {
    display: inline-flex;
    align-items: center;
    gap: 0.2rem;
    border: 0;
    border-radius: 0.3rem;
    background: transparent;
    padding: 0.15rem 0.4rem;
    color: inherit;
    font-family: inherit;
    font-size: inherit;
    cursor: pointer;
  }

  .diff-gap-action:hover {
    background: rgb(var(--color-surface-800) / 0.65);
    color: rgb(var(--theme-text-secondary));
  }

  .diff-gap-action--all {
    font-variant-numeric: tabular-nums;
  }

  .diff-side-labels,
  .diff-side-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }

  .diff-side-labels {
    position: sticky;
    top: 0;
    z-index: 2;
    border-bottom: 1px solid rgb(var(--color-surface-500) / 0.2);
    background: rgb(var(--color-surface-900) / 0.98);
    color: rgb(var(--theme-text-faint));
    font-size: 0.6875rem;
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
    grid-template-columns: 1.5rem 2.25rem 1.1rem minmax(0, 1fr);
    min-width: 0;
    background: rgb(var(--color-surface-950));
  }

  .diff-gap--side {
    grid-column: 1 / -1;
  }

  @media (max-width: 48rem) {
    .diff-view {
      max-width: 100%;
      overflow: visible;
      overscroll-behavior-x: none;
      touch-action: pan-y;
    }

    .diff-line {
      min-width: 0;
      grid-template-columns: 1.25rem 1.75rem 1.75rem 0.85rem minmax(0, 1fr);
    }

    .diff-comment-slot,
    .diff-gutter,
    .diff-marker,
    .diff-side-row > div .diff-gutter--old,
    .diff-side-row > div .diff-gutter--new,
    .diff-side-row > div .diff-marker {
      position: static;
      left: auto;
    }

    .diff-comment-slot {
      width: 1.25rem;
    }

    .diff-view :global(.diff-code) {
      white-space: pre-wrap;
      overflow-wrap: anywhere;
    }

    .diff-side-labels {
      display: none;
    }

    .diff-side-row {
      grid-template-columns: minmax(0, 1fr);
    }

    .diff-side-row > div {
      grid-template-columns: 1.25rem 1.75rem 0.85rem minmax(0, 1fr);
    }

    .diff-side-row > div + div {
      border-top: 1px solid rgb(var(--color-surface-500) / 0.18);
      border-left: 0;
    }
  }
</style>
