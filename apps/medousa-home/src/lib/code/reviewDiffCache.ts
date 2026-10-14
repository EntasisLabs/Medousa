import type { ReviewFileDiff, ReviewProjection } from "$lib/forge";
import { splitDiffFileLines } from "$lib/diff/diffTypes";

const MAX_FILES = 12;
const MAX_BYTES = 24 * 1024 * 1024;
const splitFiles = new WeakMap<ReviewFileDiff, { before: string[]; after: string[] }>();

export function reviewComparisonKey(review: ReviewProjection): string {
  return JSON.stringify([
    review.work_id,
    review.attempt_id,
    review.evidence_id,
    review.baseline_oid,
    review.sealed_head_oid,
  ]);
}

/** Reserve response space before fetching more full-file comparisons. */
export class ReviewFileLoadQueue {
  private active = 0;
  private waiters: Array<() => void> = [];

  async acquire(): Promise<() => void> {
    if (this.active >= 2) await new Promise<void>((resolve) => this.waiters.push(resolve));
    else this.active += 1;
    return () => {
      const resume = this.waiters.shift();
      if (resume) resume();
      else this.active -= 1;
    };
  }
}

/** Scope previews share the file's line arrays instead of splitting both versions again. */
export function reviewDiffLines(diff: ReviewFileDiff) {
  let lines = splitFiles.get(diff);
  if (!lines) {
    lines = {
      before: splitDiffFileLines(diff.baseline.content),
      after: splitDiffFileLines(diff.reviewed.content),
    };
    splitFiles.set(diff, lines);
  }
  return lines;
}

/** Conservative retained-data estimate; this is not a browser heap measurement. */
export function reviewDiffBytes(diff: ReviewFileDiff): number {
  let bytes = 512 + diff.path.length * 2 + diff.changed_lines.length * 64;
  for (const content of [diff.baseline.content, diff.reviewed.content]) {
    if (!content) continue;
    bytes += content.length * 4;
    // Include shared split-line contents and array/string overhead for context expansion.
    bytes += 48;
    for (let i = 0; i < content.length; i++) if (content.charCodeAt(i) === 10) bytes += 48;
  }
  for (const hunk of diff.hunks) {
    bytes += 128;
    for (const line of hunk.lines) bytes += 128 + line.content.length * 2;
  }
  return bytes;
}

/** Least-recently-used open comparisons. The newest file always remains readable. */
export class ReviewDiffCache {
  private entries = new Map<string, { diff: ReviewFileDiff; bytes: number }>();
  private bytes = 0;

  constructor(private maxBytes = MAX_BYTES, private maxFiles = MAX_FILES) {}

  get(path: string): ReviewFileDiff | undefined {
    const entry = this.entries.get(path);
    if (!entry) return undefined;
    this.entries.delete(path);
    this.entries.set(path, entry);
    return entry.diff;
  }

  set(path: string, diff: ReviewFileDiff): string[] {
    this.delete(path);
    const bytes = reviewDiffBytes(diff);
    this.entries.set(path, { diff, bytes });
    this.bytes += bytes;
    const evicted: string[] = [];
    // A single oversized response is retained instead of making that file inaccessible.
    // The daemon already bounds individual file versions and patches.
    while (this.entries.size > 1 && (this.bytes > this.maxBytes || this.entries.size > this.maxFiles)) {
      const oldest = this.entries.keys().next().value!;
      this.delete(oldest);
      evicted.push(oldest);
    }
    return evicted;
  }

  delete(path: string): void {
    const entry = this.entries.get(path);
    if (entry) this.bytes -= entry.bytes;
    this.entries.delete(path);
  }

  clear(): void {
    this.entries.clear();
    this.bytes = 0;
  }

  snapshot(): Record<string, ReviewFileDiff> {
    return Object.fromEntries([...this.entries].map(([path, entry]) => [path, entry.diff]));
  }
}
