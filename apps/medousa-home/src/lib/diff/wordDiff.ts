import type { DiffLine } from "./diffTypes";

export type WordPart = {
  text: string;
  changed: boolean;
};

export type SideRow = {
  key: string;
  oldNumber?: number | null;
  newNumber?: number | null;
  oldContent: string;
  newContent: string;
  kind: "context" | "addition" | "deletion" | "replacement";
  oldParts?: WordPart[];
  newParts?: WordPart[];
};

/** Tokenize for word-level diff — words, whitespace, and punctuation. */
export function tokenizeForWordDiff(text: string): string[] {
  return text.match(/\w+|[^\w\s]|\s+/g) ?? (text ? [text] : []);
}

function lcsTable(a: string[], b: string[]): number[][] {
  const rows = a.length + 1;
  const cols = b.length + 1;
  const table: number[][] = Array.from({ length: rows }, () => Array(cols).fill(0));
  for (let i = 1; i < rows; i += 1) {
    for (let j = 1; j < cols; j += 1) {
      table[i]![j] =
        a[i - 1] === b[j - 1]
          ? table[i - 1]![j - 1]! + 1
          : Math.max(table[i - 1]![j]!, table[i]![j - 1]!);
    }
  }
  return table;
}

/**
 * Word/char-level diff of two strings. Unchanged tokens keep `changed: false`;
 * insertions/deletions are marked changed on the appropriate side.
 */
export function wordDiffParts(before: string, after: string): {
  before: WordPart[];
  after: WordPart[];
} {
  const a = tokenizeForWordDiff(before);
  const b = tokenizeForWordDiff(after);
  if (a.length === 0 && b.length === 0) return { before: [], after: [] };
  if (a.join("") === b.join("")) {
    return {
      before: a.map((text) => ({ text, changed: false })),
      after: b.map((text) => ({ text, changed: false })),
    };
  }

  // Prefer character LCS for short single-token changes.
  const useChars =
    a.length <= 1 && b.length <= 1 && Math.max(before.length, after.length) <= 80;
  const left = useChars ? [...before] : a;
  const right = useChars ? [...after] : b;
  const table = lcsTable(left, right);
  const beforeParts: WordPart[] = [];
  const afterParts: WordPart[] = [];
  let i = left.length;
  let j = right.length;
  const reverseBefore: WordPart[] = [];
  const reverseAfter: WordPart[] = [];
  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && left[i - 1] === right[j - 1]) {
      reverseBefore.push({ text: left[i - 1]!, changed: false });
      reverseAfter.push({ text: right[j - 1]!, changed: false });
      i -= 1;
      j -= 1;
    } else if (j > 0 && (i === 0 || table[i]![j - 1]! >= table[i - 1]![j]!)) {
      reverseAfter.push({ text: right[j - 1]!, changed: true });
      j -= 1;
    } else if (i > 0) {
      reverseBefore.push({ text: left[i - 1]!, changed: true });
      i -= 1;
    }
  }
  beforeParts.push(...reverseBefore.reverse());
  afterParts.push(...reverseAfter.reverse());
  return { before: beforeParts, after: afterParts };
}

/** Dice / bigram similarity in [0, 1]. */
export function lineSimilarity(a: string, b: string): number {
  if (a === b) return 1;
  if (!a.length || !b.length) return 0;
  if (a.length === 1 && b.length === 1) return a === b ? 1 : 0;
  const bigrams = (value: string): Map<string, number> => {
    const map = new Map<string, number>();
    for (let i = 0; i < value.length - 1; i += 1) {
      const gram = value.slice(i, i + 2);
      map.set(gram, (map.get(gram) ?? 0) + 1);
    }
    return map;
  };
  const left = bigrams(a);
  const right = bigrams(b);
  let overlap = 0;
  for (const [gram, count] of left) {
    overlap += Math.min(count, right.get(gram) ?? 0);
  }
  return (2 * overlap) / Math.max(1, a.length + b.length - 2);
}

const PAIR_THRESHOLD = 0.35;

/**
 * Pair deletions with additions by similarity instead of positional zip.
 * Unmatched lines sit opposite blanks.
 */
export function pairSideRows(
  hunkKey: string,
  deletions: DiffLine[],
  additions: DiffLine[],
): SideRow[] {
  const usedAdds = new Set<number>();
  const pairs: Array<{ del: DiffLine | null; add: DiffLine | null; score: number }> = [];

  for (const del of deletions) {
    let bestIndex = -1;
    let bestScore = PAIR_THRESHOLD;
    for (let ai = 0; ai < additions.length; ai += 1) {
      if (usedAdds.has(ai)) continue;
      const score = lineSimilarity(del.content, additions[ai]!.content);
      if (score > bestScore) {
        bestScore = score;
        bestIndex = ai;
      }
    }
    if (bestIndex >= 0) {
      usedAdds.add(bestIndex);
      pairs.push({ del, add: additions[bestIndex]!, score: bestScore });
    } else {
      pairs.push({ del, add: null, score: 0 });
    }
  }
  for (let ai = 0; ai < additions.length; ai += 1) {
    if (!usedAdds.has(ai)) pairs.push({ del: null, add: additions[ai]!, score: 0 });
  }

  // Stable visual order: keep relative order of deletions, then unmatched additions.
  const ordered: typeof pairs = [];
  const unmatchedAdds: typeof pairs = [];
  for (const pair of pairs) {
    if (pair.del) ordered.push(pair);
    else unmatchedAdds.push(pair);
  }
  ordered.push(...unmatchedAdds);

  return ordered.map((pair, offset) => {
    const oldContent = pair.del?.content ?? "";
    const newContent = pair.add?.content ?? "";
    const kind =
      pair.del && pair.add ? "replacement" : pair.del ? "deletion" : "addition";
    const parts =
      kind === "replacement" ? wordDiffParts(oldContent, newContent) : null;
    return {
      key: `${hunkKey}:change:${offset}`,
      oldNumber: pair.del?.old_line,
      newNumber: pair.add?.new_line,
      oldContent,
      newContent,
      kind,
      oldParts: parts?.before,
      newParts: parts?.after,
    };
  });
}

/**
 * Build side-by-side rows for a hunk with similarity pairing and word parts.
 */
export function sideRowsForHunk(
  hunkKey: string,
  lines: DiffLine[],
): SideRow[] {
  const rows: SideRow[] = [];
  for (let index = 0; index < lines.length; ) {
    const line = lines[index]!;
    if (line.kind === "context") {
      rows.push({
        key: `${hunkKey}:context:${index}`,
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
    while (index < lines.length && lines[index]!.kind !== "context") {
      block.push(lines[index]!);
      index += 1;
    }
    const deletions = block.filter((entry) => entry.kind === "deletion");
    const additions = block.filter((entry) => entry.kind === "addition");
    rows.push(...pairSideRows(`${hunkKey}:${index}`, deletions, additions));
  }
  return rows;
}
