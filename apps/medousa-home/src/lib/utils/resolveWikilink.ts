import type { VaultNote } from "$lib/types/vault";

function filenameStem(path: string): string {
  const base = path.split("/").pop() ?? path;
  return base.replace(/\.md$/i, "");
}

function normalizePath(raw: string): string {
  const trimmed = raw.trim().replace(/^\.\//, "").replace(/\/+/g, "/");
  const withExt = trimmed.endsWith(".md") ? trimmed : `${trimmed}.md`;
  return withExt.replace(/^\//, "");
}

/** Client-side wikilink resolution (mirrors daemon index heuristics). */
export function resolveWikilinkTarget(
  raw: string,
  sourcePath: string | null,
  notes: VaultNote[],
): string | null {
  const token = raw.split("#")[0]?.split("|")[0]?.trim() ?? "";
  if (!token) return null;

  const knownPaths = notes.map((note) => note.path);
  const known = new Set(knownPaths);

  const candidates: string[] = [];

  if (token.includes("/")) {
    candidates.push(normalizePath(token));
  } else {
    const stem = filenameStem(token);
    const sourceDir = sourcePath?.includes("/")
      ? sourcePath.slice(0, sourcePath.lastIndexOf("/"))
      : "";
    if (sourceDir) {
      candidates.push(`${sourceDir}/${stem}.md`);
    }
    candidates.push(`${stem}.md`);

    for (const path of knownPaths) {
      if (filenameStem(path) === stem) {
        candidates.push(path);
      }
    }

    const tokenLower = stem.toLowerCase();
    for (const note of notes) {
      const titleStem = note.title.trim().toLowerCase();
      if (titleStem === tokenLower || titleStem.includes(tokenLower)) {
        candidates.push(note.path);
      }
    }
  }

  for (const candidate of candidates) {
    const normalized = normalizePath(candidate);
    if (known.has(normalized)) return normalized;
  }

  return candidates.map(normalizePath).find((path) => known.has(path)) ?? null;
}
