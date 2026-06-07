export interface LineDiffStats {
  added: number;
  removed: number;
  changed: number;
}

/** Lightweight line diff for editor chips — not a full diff view. */
export function lineDiffStats(before: string, after: string): LineDiffStats {
  const beforeLines = before.split("\n");
  const afterLines = after.split("\n");
  const maxLen = Math.max(beforeLines.length, afterLines.length);
  let added = 0;
  let removed = 0;
  let changed = 0;

  for (let i = 0; i < maxLen; i += 1) {
    const left = beforeLines[i];
    const right = afterLines[i];
    if (left === undefined) {
      added += 1;
    } else if (right === undefined) {
      removed += 1;
    } else if (left !== right) {
      changed += 1;
    }
  }

  return { added, removed, changed };
}

export function formatDiffChip(stats: LineDiffStats): string {
  const parts: string[] = [];
  if (stats.added > 0) parts.push(`+${stats.added}`);
  if (stats.removed > 0) parts.push(`-${stats.removed}`);
  if (stats.changed > 0 && parts.length === 0) parts.push(`~${stats.changed}`);
  return parts.join(" ") || "edited";
}
