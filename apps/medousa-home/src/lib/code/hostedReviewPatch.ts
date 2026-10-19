import type { DiffHunk } from "$lib/diff/diffTypes";

/** Parse the provider's unified patch; retain original old/new line numbers. */
export function hostedReviewHunks(patch: string): DiffHunk[] {
  const hunks: DiffHunk[] = [];
  let oldLine = 0, newLine = 0;
  for (const line of patch.split("\n")) {
    const header = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(line);
    if (header) {
      oldLine = Number(header[1]); newLine = Number(header[3]);
      hunks.push({ old_start: oldLine, old_count: Number(header[2] ?? 1), new_start: newLine, new_count: Number(header[4] ?? 1), lines: [] });
      continue;
    }
    const hunk = hunks.at(-1);
    if (!hunk || ![" ", "+", "-"].includes(line[0])) continue;
    const kind = line[0] === "+" ? "addition" : line[0] === "-" ? "deletion" : "context";
    hunk.lines.push({ kind, content: line.slice(1), old_line: kind === "addition" ? null : oldLine++, new_line: kind === "deletion" ? null : newLine++ });
  }
  return hunks;
}
