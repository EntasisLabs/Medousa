export type DiffLineKind = "context" | "addition" | "deletion" | string;

export type DiffLine = {
  kind: DiffLineKind;
  old_line?: number | null;
  new_line?: number | null;
  content: string;
};

export type DiffHunk = {
  old_start: number;
  old_count: number;
  new_start: number;
  new_count: number;
  lines: DiffLine[];
};

export type DiffFileSection = {
  /** Stable UI identity when multiple operations resolve to the same final path. */
  id?: string;
  path: string;
  oldPath?: string | null;
  status?: string;
  binary?: boolean;
  additions?: number;
  deletions?: number;
  hunks: DiffHunk[];
  /** Optional binary metadata for empty/binary display */
  baselineBytes?: number | null;
  reviewedBytes?: number | null;
  baselineExists?: boolean;
  reviewedExists?: boolean;
  /** Full before/after text for real context expansion between hunks. */
  beforeText?: string | null;
  afterText?: string | null;
  conflict?: boolean;
};

/** Split file text into lines without dropping a trailing empty line marker. */
export function splitDiffFileLines(text: string | null | undefined): string[] {
  if (text == null || text === "") return [];
  const lines = text.split("\n");
  if (text.endsWith("\n")) lines.pop();
  return lines;
}

/**
 * Unmodified lines spanning `[startLine, endLine]` (1-based inclusive) from a
 * file's line array. Returns empty when the range is invalid.
 */
export function linesInRange(
  lines: string[],
  startLine: number,
  endLine: number,
): Array<{ line: number; content: string }> {
  if (startLine < 1 || endLine < startLine) return [];
  const out: Array<{ line: number; content: string }> = [];
  for (let line = startLine; line <= endLine; line += 1) {
    const content = lines[line - 1];
    if (content === undefined) break;
    out.push({ line, content });
  }
  return out;
}

export function countDiffStats(hunks: DiffHunk[]): { additions: number; deletions: number } {
  let additions = 0;
  let deletions = 0;
  for (const hunk of hunks) {
    for (const line of hunk.lines) {
      if (line.kind === "addition") additions += 1;
      else if (line.kind === "deletion") deletions += 1;
    }
  }
  return { additions, deletions };
}

export function countStackStats(files: DiffFileSection[]): {
  files: number;
  additions: number;
  deletions: number;
} {
  let additions = 0;
  let deletions = 0;
  for (const file of files) {
    if (typeof file.additions === "number" && typeof file.deletions === "number") {
      additions += file.additions;
      deletions += file.deletions;
      continue;
    }
    const stats = countDiffStats(file.hunks);
    additions += file.additions ?? stats.additions;
    deletions += file.deletions ?? stats.deletions;
  }
  return { files: files.length, additions, deletions };
}
