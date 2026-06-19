import type { VaultNote } from "$lib/types/vault";
import { normalizeVaultNotePath } from "$lib/utils/vaultNoteTitle";

function filenameStem(path: string): string {
  const base = path.split("/").pop() ?? path;
  return base.replace(/\.md$/i, "");
}

export function parseWikilinkTarget(raw: string): {
  pathToken: string;
  heading: string | null;
} {
  const decoded = raw.trim();
  const hashIndex = decoded.indexOf("#");
  if (hashIndex === -1) {
    return { pathToken: decoded, heading: null };
  }
  const pathToken = decoded.slice(0, hashIndex).trim();
  const heading = decoded.slice(hashIndex + 1).trim();
  return {
    pathToken,
    heading: heading || null,
  };
}

/** Suggested vault path for an unresolved wikilink token. */
export function suggestPathForWikilinkToken(
  raw: string,
  sourcePath: string | null,
): string {
  const { pathToken } = parseWikilinkTarget(raw);
  const token = pathToken || raw.trim();
  if (token.includes("/")) {
    return normalizeVaultNotePath(token);
  }
  const stem = filenameStem(token);
  const sourceDir = sourcePath?.includes("/")
    ? sourcePath.slice(0, sourcePath.lastIndexOf("/"))
    : "";
  if (sourceDir) {
    return normalizeVaultNotePath(`${sourceDir}/${stem}`);
  }
  return normalizeVaultNotePath(stem);
}

/** Client-side wikilink resolution (mirrors daemon index heuristics). */
export function resolveWikilinkTarget(
  raw: string,
  sourcePath: string | null,
  notes: VaultNote[],
): string | null {
  const { pathToken } = parseWikilinkTarget(raw);
  const token = pathToken || (raw.split("#")[0]?.split("|")[0]?.trim() ?? "");
  if (!token) return null;

  const knownPaths = notes.map((note) => note.path);
  const known = new Set(knownPaths);

  const candidates: string[] = [];

  if (token.includes("/")) {
    candidates.push(normalizeVaultNotePath(token));
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
    const normalized = normalizeVaultNotePath(candidate);
    if (known.has(normalized)) return normalized;
  }

  return candidates.map(normalizeVaultNotePath).find((path) => known.has(path)) ?? null;
}
