/**
 * Parse workshop-relative file locations from a terminal line for clickable links.
 */

export type TerminalFileLinkHit = {
  startIndex: number;
  length: number;
  path: string;
  line: number | null;
  column: number | null;
};

const PATH_WITH_LOC =
  /(?:^|[\s("'`])((?:\.\/|\.\.\/|[A-Za-z]:[\\/]|\/)?(?:[\w.@+-]+[\\/])*[\w.@+-]+\.[A-Za-z0-9]+)(?::(\d+))?(?::(\d+))?/g;

function normalizeSlashes(path: string): string {
  return path.replace(/\\/g, "/");
}

function stripWorktreePrefix(path: string, worktreeRoot: string | null | undefined): string | null {
  const normalized = normalizeSlashes(path);
  const root = worktreeRoot ? normalizeSlashes(worktreeRoot).replace(/\/+$/, "") : "";
  if (root && (normalized === root || normalized.startsWith(`${root}/`))) {
    const relative = normalized.slice(root.length).replace(/^\//, "");
    return relative || null;
  }
  if (normalized.startsWith("/")) {
    // Absolute path outside the worktree is not a Code buffer target.
    return null;
  }
  return normalized.replace(/^\.\//, "");
}

/**
 * Find path[:line[:column]] tokens suitable for opening in Code.
 * Requires a file extension so bare words like `error:` are ignored.
 */
export function parseTerminalFileLinks(
  line: string,
  worktreeRoot?: string | null,
): TerminalFileLinkHit[] {
  const hits: TerminalFileLinkHit[] = [];
  PATH_WITH_LOC.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = PATH_WITH_LOC.exec(line)) != null) {
    const rawPath = match[1];
    if (!rawPath) continue;
    const absoluteStart = match.index + match[0].indexOf(rawPath);
    if (absoluteStart < 0) continue;
    const path = stripWorktreePrefix(rawPath, worktreeRoot);
    if (!path || path.includes("..")) continue;
    const lineNo = match[2] ? Number.parseInt(match[2], 10) : null;
    const column = match[3] ? Number.parseInt(match[3], 10) : null;
    const length =
      rawPath.length +
      (match[2] ? match[2].length + 1 : 0) +
      (match[3] ? match[3].length + 1 : 0);
    hits.push({
      startIndex: absoluteStart,
      length,
      path,
      line: Number.isFinite(lineNo) && (lineNo ?? 0) > 0 ? lineNo : null,
      column: Number.isFinite(column) && (column ?? 0) > 0 ? column : null,
    });
  }
  return hits;
}
