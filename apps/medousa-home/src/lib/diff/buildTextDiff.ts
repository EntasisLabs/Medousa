import type { DiffHunk, DiffLine } from "./diffTypes";

const CONTEXT = 3;

type Edit =
  | { kind: "equal"; oldIndex: number; newIndex: number; content: string }
  | { kind: "delete"; oldIndex: number; content: string }
  | { kind: "insert"; newIndex: number; content: string };

function splitLines(text: string): string[] {
  if (text.length === 0) return [];
  const normalized = text.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  const lines = normalized.split("\n");
  if (normalized.endsWith("\n")) lines.pop();
  return lines;
}

/** Myers O(ND) line diff → edit script. */
function myersEdits(a: string[], b: string[]): Edit[] {
  const n = a.length;
  const m = b.length;
  if (n === 0 && m === 0) return [];
  if (n === 0) {
    return b.map((content, newIndex) => ({ kind: "insert" as const, newIndex, content }));
  }
  if (m === 0) {
    return a.map((content, oldIndex) => ({ kind: "delete" as const, oldIndex, content }));
  }

  const max = n + m;
  const offset = max;
  const v = new Int32Array(2 * max + 1);
  v.fill(Number.MIN_SAFE_INTEGER);
  v[offset + 1] = 0;
  const trace: Int32Array[] = [];

  let foundD = -1;
  for (let d = 0; d <= max; d += 1) {
    for (let k = -d; k <= d; k += 2) {
      let x: number;
      if (k === -d || (k !== d && v[offset + k - 1] < v[offset + k + 1])) {
        x = v[offset + k + 1];
      } else {
        x = v[offset + k - 1] + 1;
      }
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) {
        x += 1;
        y += 1;
      }
      v[offset + k] = x;
      if (x >= n && y >= m) {
        foundD = d;
        break;
      }
    }
    trace.push(new Int32Array(v));
    if (foundD >= 0) break;
  }

  if (foundD < 0) {
    return [
      ...a.map((content, oldIndex) => ({ kind: "delete" as const, oldIndex, content })),
      ...b.map((content, newIndex) => ({ kind: "insert" as const, newIndex, content })),
    ];
  }

  const edits: Edit[] = [];
  let x = n;
  let y = m;

  for (let d = foundD; d > 0; d -= 1) {
    const vPrev = trace[d - 1]!;
    const k = x - y;
    let prevK: number;
    if (k === -d || (k !== d && vPrev[offset + k - 1] < vPrev[offset + k + 1])) {
      prevK = k + 1;
    } else {
      prevK = k - 1;
    }
    const prevX = vPrev[offset + prevK]!;
    const prevY = prevX - prevK;

    while (x > prevX && y > prevY) {
      x -= 1;
      y -= 1;
      edits.push({ kind: "equal", oldIndex: x, newIndex: y, content: a[x]! });
    }

    if (x === prevX) {
      y -= 1;
      edits.push({ kind: "insert", newIndex: y, content: b[y]! });
    } else {
      x -= 1;
      edits.push({ kind: "delete", oldIndex: x, content: a[x]! });
    }
  }

  while (x > 0 && y > 0) {
    x -= 1;
    y -= 1;
    edits.push({ kind: "equal", oldIndex: x, newIndex: y, content: a[x]! });
  }
  while (x > 0) {
    x -= 1;
    edits.push({ kind: "delete", oldIndex: x, content: a[x]! });
  }
  while (y > 0) {
    y -= 1;
    edits.push({ kind: "insert", newIndex: y, content: b[y]! });
  }

  edits.reverse();
  return edits;
}

function groupIntoHunks(edits: Edit[], context: number): DiffHunk[] {
  if (edits.length === 0) return [];

  const changeSpans: Array<{ start: number; end: number }> = [];
  for (let i = 0; i < edits.length; i += 1) {
    if (edits[i]!.kind === "equal") continue;
    const start = i;
    while (i + 1 < edits.length && edits[i + 1]!.kind !== "equal") i += 1;
    changeSpans.push({ start, end: i });
  }
  if (changeSpans.length === 0) return [];

  const regions: Array<{ start: number; end: number }> = [];
  for (const change of changeSpans) {
    const start = Math.max(0, change.start - context);
    const end = Math.min(edits.length - 1, change.end + context);
    const last = regions[regions.length - 1];
    if (last && start <= last.end + 1) {
      last.end = Math.max(last.end, end);
    } else {
      regions.push({ start, end });
    }
  }

  return regions.map((region) => {
    const lines: DiffLine[] = [];
    let oldCount = 0;
    let newCount = 0;

    for (let i = region.start; i <= region.end; i += 1) {
      const edit = edits[i]!;
      if (edit.kind === "equal") {
        lines.push({
          kind: "context",
          old_line: edit.oldIndex + 1,
          new_line: edit.newIndex + 1,
          content: edit.content,
        });
        oldCount += 1;
        newCount += 1;
      } else if (edit.kind === "delete") {
        lines.push({
          kind: "deletion",
          old_line: edit.oldIndex + 1,
          new_line: null,
          content: edit.content,
        });
        oldCount += 1;
      } else {
        lines.push({
          kind: "addition",
          old_line: null,
          new_line: edit.newIndex + 1,
          content: edit.content,
        });
        newCount += 1;
      }
    }

    const firstOld = lines.find((l) => l.old_line != null)?.old_line;
    const firstNew = lines.find((l) => l.new_line != null)?.new_line;

    return {
      old_start: typeof firstOld === "number" ? firstOld : 0,
      old_count: oldCount,
      new_start: typeof firstNew === "number" ? firstNew : 0,
      new_count: newCount,
      lines,
    };
  });
}

/**
 * Build DiffHunk[] from two text strings using Myers line diff.
 * Consecutive changes are grouped into hunks with ~3 context lines.
 */
export function buildTextDiff(before: string, after: string): DiffHunk[] {
  const a = splitLines(before);
  const b = splitLines(after);
  if (a.length === 0 && b.length === 0) return [];
  const edits = myersEdits(a, b);
  return groupIntoHunks(edits, CONTEXT);
}
