import type { VaultNote } from "$lib/types/vault";
import { normalizeVaultNotePath } from "$lib/utils/vaultNoteTitle";
import {
  buildVaultLookupSnapshot,
  type VaultLookupSnapshot,
  resolveWikilinkWithLookup,
} from "$lib/utils/vaultLookup";

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

/**
 * Client-side wikilink resolution.
 * Prefer passing a VaultLookupSnapshot (O(L) map probes). The VaultNote[]
 * overload remains as a thin adapter for migration.
 */
export function resolveWikilinkTarget(
  raw: string,
  sourcePath: string | null,
  notesOrLookup: VaultNote[] | VaultLookupSnapshot,
): string | null {
  const { pathToken } = parseWikilinkTarget(raw);
  const token = pathToken || (raw.split("#")[0]?.split("|")[0]?.trim() ?? "");
  if (!token) return null;

  const lookup = Array.isArray(notesOrLookup)
    ? buildVaultLookupSnapshot(notesOrLookup, 0, sourcePath)
    : notesOrLookup;

  const result = resolveWikilinkWithLookup(token, sourcePath, lookup);
  return result.kind === "resolved" ? result.path : null;
}
